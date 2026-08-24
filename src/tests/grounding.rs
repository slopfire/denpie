use crate::db::repositories::{documents, tipcards, topics};
use crate::tests::support::setup_db;

/// Fresh PostgreSQL migrations create the grounding columns and GIN-backed search table.
#[tokio::test]
async fn migrations_create_postgres_grounding_schema() {
    let pool = setup_db().await;

    for (table, column) in [
        ("topics", "grounding_strategy"),
        ("topics", "grounding_model"),
        ("topics", "grounding_reasoning_effort"),
        ("topics", "image_strategy"),
        ("tipcards", "use_image"),
        ("tipcards", "image_query"),
        ("review_states", "feedback"),
        ("review_states", "reviewed_at"),
        ("user_settings", "grounding_strategy"),
        ("user_settings", "llm_grounding_model"),
        ("user_settings", "llm_grounding_reasoning_effort"),
        ("user_settings", "image_sources"),
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM information_schema.columns
             WHERE table_schema = current_schema() AND table_name = $1 AND column_name = $2",
        )
        .bind(table)
        .bind(column)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "missing {table}.{column}");
    }

    let index_definition: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes
         WHERE schemaname = current_schema() AND indexname = 'idx_document_chunks_fts'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(index_definition.contains("USING gin"));
    assert!(index_definition.contains("to_tsvector"));

    let topic_id: i64 = sqlx::query_scalar(
        "INSERT INTO topics (user_id, name, tipcard_type)
         VALUES ('usr_test_admin', 'migration defaults', 'repeatable_tip')
         RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let image_defaults: (i64, String) = sqlx::query_as(
        "INSERT INTO tipcards (user_id, topic_id, full_content, compressed_content)
         VALUES ('usr_test_admin', $1, 'card', 'card')
         RETURNING use_image, image_query",
    )
    .bind(topic_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(image_defaults, (0, String::new()));
}

/// RAG retrieval is isolated to explicit topic assignments and supports reuse.
#[tokio::test]
async fn rag_retrieval_returns_matching_chunk() {
    let pool = setup_db().await;
    let user = "rag_user";
    sqlx::query(
        "INSERT INTO users (id, username, role) VALUES ($1, 'rag', 'user')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    // A topic owned by this user.
    let topic_id: i64 = sqlx::query_scalar(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ($1, 'borrow', 'repeatable_tip') RETURNING id",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap();

    documents::insert_document(
        &pool,
        user,
        &[topic_id],
        "document",
        "Rust borrow checker",
        None,
        "The borrow checker allows one mutable or many immutable borrows. You cannot have both at once.",
    )
    .await
    .unwrap();

    let other_topic_id: i64 = sqlx::query_scalar(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ($1, 'other', 'repeatable_tip') RETURNING id",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap();
    let shared_id = documents::insert_document(
        &pool,
        user,
        &[topic_id, other_topic_id],
        "document",
        "Shared source",
        None,
        "borrow checker shared by two topics",
    )
    .await
    .unwrap();
    documents::insert_document(
        &pool,
        user,
        &[],
        "link",
        "Global linked site",
        Some("https://example.com/rust"),
        "borrow checker from a globally assigned linked site",
    )
    .await
    .unwrap();

    let listed = documents::list_documents(&pool, user, None).await.unwrap();
    let listed_shared = listed.iter().find(|doc| doc.id == shared_id).unwrap();
    assert_eq!(listed_shared.topic_ids, vec![topic_id, other_topic_id]);

    let hits = documents::retrieve_chunks(&pool, user, topic_id, "borrow checker", 5)
        .await
        .unwrap();
    assert_eq!(
        hits.len(),
        2,
        "topic retrieval includes assigned sources only"
    );
    assert!(hits.iter().any(|hit| hit.contains("shared by two topics")));
    assert!(!hits.iter().any(|hit| hit.contains("globally assigned")));

    let other_hits = documents::retrieve_chunks(&pool, user, other_topic_id, "borrow checker", 5)
        .await
        .unwrap();
    assert_eq!(
        other_hits.len(),
        1,
        "shared source is reusable across topics"
    );

    documents::detach_document_topic(&pool, user, shared_id, topic_id)
        .await
        .unwrap();
    let detached_hits = documents::retrieve_chunks(&pool, user, topic_id, "borrow checker", 5)
        .await
        .unwrap();
    assert_eq!(
        detached_hits.len(),
        1,
        "detaching removes source from retrieval"
    );
    documents::attach_document_topic(&pool, user, shared_id, topic_id)
        .await
        .unwrap();
    let reattached = documents::list_documents(&pool, user, Some(topic_id))
        .await
        .unwrap();
    assert!(reattached.iter().any(|doc| doc.id == shared_id));
    assert!(
        documents::detach_document_topic(&pool, "other_user", shared_id, topic_id)
            .await
            .is_err()
    );

    // A query with no term overlap returns nothing.
    let misses = documents::retrieve_chunks(&pool, user, topic_id, "quilting patterns", 5)
        .await
        .unwrap();
    assert!(misses.is_empty(), "expected no matches");
}

/// Pending backlog: pending cards are promoted oldest-first and flipped active.
#[tokio::test]
async fn pending_backlog_promotes_oldest_first() {
    let pool = setup_db().await;
    let user = "pending_user";
    sqlx::query(
        "INSERT INTO users (id, username, role) VALUES ($1, 'pending', 'user')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    let topic_id: i64 = sqlx::query_scalar(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ($1, 'agg', 'repeatable_tip') RETURNING id",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Two pending cards; sleep so created_at ordering is deterministic.
    let first = tipcards::create_generated_with_status(
        &pool,
        user,
        topic_id,
        "repeatable_tip",
        "First",
        "c1",
        "c1",
        true,
        "first diagram",
        "pending",
    )
    .await
    .unwrap();
    // Backdate the second card so the first is provably older.
    let second = tipcards::create_generated_with_status(
        &pool,
        user,
        topic_id,
        "repeatable_tip",
        "Second",
        "c2",
        "c2",
        false,
        "",
        "pending",
    )
    .await
    .unwrap();
    sqlx::query("UPDATE tipcards SET created_at = '2000-01-01 00:00:00' WHERE id = $1")
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE tipcards SET created_at = '2001-01-01 00:00:00' WHERE id = $1")
        .bind(second)
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(
        tipcards::count_pending(&pool, user, topic_id, "repeatable_tip")
            .await
            .unwrap(),
        2
    );

    let topic_summary = topics::list_app_topics(&pool, user, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(topic_summary.len(), 1);
    assert_eq!(topic_summary[0].pending_cards, 2);

    let card_a = tipcards::take_pending_card(&pool, user, topic_id, "repeatable_tip")
        .await
        .unwrap()
        .expect("first pending card");
    assert_eq!(card_a.id, first);
    assert!(card_a.use_image);
    assert_eq!(card_a.image_query, "first diagram");
    let card_b = tipcards::take_pending_card(&pool, user, topic_id, "repeatable_tip")
        .await
        .unwrap()
        .expect("second pending card");
    assert_eq!(card_b.id, second);
    assert!(!card_b.use_image);
    assert!(card_b.image_query.is_empty());

    // Both are now active; no more pending cards.
    let status: String = sqlx::query_scalar("SELECT status FROM review_states WHERE card_id = $1")
        .bind(first)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "active");
    assert_eq!(
        tipcards::count_pending(&pool, user, topic_id, "repeatable_tip")
            .await
            .unwrap(),
        0
    );
    let topic_summary = topics::list_app_topics(&pool, user, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(topic_summary[0].pending_cards, 0);

    assert!(
        tipcards::take_pending_card(&pool, user, topic_id, "repeatable_tip")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn pending_only_topic_promotes_one_card_for_page_load() {
    let pool = setup_db().await;
    let user = "reload_pending_user";
    sqlx::query(
        "INSERT INTO users (id, username, role) VALUES ($1, 'reload-pending', 'user')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    let topic_id: i64 = sqlx::query_scalar(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ($1, 'reload deck', 'repeatable_tip') RETURNING id",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap();
    for index in 0..3 {
        tipcards::create_generated_with_status(
            &pool,
            user,
            topic_id,
            "repeatable_tip",
            &format!("Card {index}"),
            &format!("Full {index}"),
            &format!("Compact {index}"),
            false,
            "",
            "pending",
        )
        .await
        .unwrap();
    }

    tipcards::promote_pending_within_daily_limits(
        &pool,
        user,
        &[tipcards::DailyReviewTarget {
            topic_id,
            window_start: chrono::Utc::now() - chrono::Duration::days(1),
            daily_card_count: 1,
        }],
    )
    .await
    .unwrap();

    let (active, pending) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT
            SUM(CASE WHEN r.status = 'active' THEN 1 ELSE 0 END),
            SUM(CASE WHEN r.status = 'pending' THEN 1 ELSE 0 END)
         FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         WHERE t.user_id = $1",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active, 1);
    assert_eq!(pending, 2);
}
