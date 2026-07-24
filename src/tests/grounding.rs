use crate::db::repositories::{documents, tipcards, topics};
use crate::tests::support::setup_db;
use sqlx::Row;

/// An old-shape database (pre-grounding columns/tables) still migrates cleanly:
/// the new columns land and the FTS5 `document_chunks` table becomes usable.
#[tokio::test]
async fn migration_adds_grounding_schema_to_old_db() {
    // Build an old-shape DB: schema.sql already includes the new columns (fresh
    // install path), so simulate an *old* DB by creating the legacy tables only
    // and then running migrations against them.
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();

    // Legacy topics (no grounding_strategy/image_strategy) and user_settings
    // (no new columns). Migrations must add them idempotently.
    sqlx::query(
        "CREATE TABLE users (id TEXT PRIMARY KEY, username TEXT NOT NULL UNIQUE, password_hash TEXT, role TEXT NOT NULL DEFAULT 'user', created_at DATETIME DEFAULT CURRENT_TIMESTAMP)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE topics (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT NOT NULL,
            name TEXT NOT NULL,
            tipcard_type TEXT NOT NULL DEFAULT 'repeatable_tip',
            prompt_template TEXT,
            daily_card_count INTEGER,
            daily_time_zone TEXT,
            daily_update_time TEXT,
            compression_level TEXT,
            icon_id TEXT,
            color_hue INTEGER,
            UNIQUE(user_id, name)
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE tipcards (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT,
            topic_id INTEGER NOT NULL,
            tipcard_type TEXT NOT NULL DEFAULT 'repeatable_tip',
            title TEXT,
            full_content TEXT NOT NULL,
            compressed_content TEXT NOT NULL,
            image_data TEXT NOT NULL DEFAULT '[]',
            pinned INTEGER NOT NULL DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE review_states (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            card_id INTEGER NOT NULL UNIQUE,
            algorithm_used TEXT NOT NULL,
            state_data TEXT NOT NULL,
            repeats INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'active',
            daily_refreshed_at DATETIME,
            next_review_at DATETIME NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE user_settings (
            user_id TEXT PRIMARY KEY,
            llm_model TEXT NOT NULL,
            llm_compress_model TEXT NOT NULL,
            prompt_template TEXT NOT NULL,
            llm_api_key TEXT NOT NULL,
            llm_base_url TEXT NOT NULL,
            llm_compress_base_url TEXT NOT NULL,
            llm_reasoning_effort TEXT NOT NULL,
            llm_compress_reasoning_effort TEXT NOT NULL,
            llm_compression_level TEXT NOT NULL,
            color_scheme TEXT NOT NULL,
            transparency TEXT NOT NULL,
            blur_intensity TEXT NOT NULL,
            daily_time_zone TEXT NOT NULL,
            daily_update_time TEXT NOT NULL,
            max_active_cards INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE api_keys (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT NOT NULL,
            key_hash TEXT NOT NULL UNIQUE,
            client_name TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE llm_token_usage (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT,
            model TEXT NOT NULL,
            purpose TEXT NOT NULL,
            prompt_tokens INTEGER NOT NULL DEFAULT 0,
            completion_tokens INTEGER NOT NULL DEFAULT 0,
            total_tokens INTEGER NOT NULL DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE daily_refresh_runs (
            user_id TEXT NOT NULL,
            topic_id INTEGER NOT NULL,
            tipcard_type TEXT NOT NULL,
            window_start DATETIME NOT NULL,
            refreshed_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(user_id, topic_id, tipcard_type)
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE tipcard_images (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT NOT NULL,
            card_id INTEGER NOT NULL,
            position INTEGER NOT NULL,
            storage_path TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            byte_size INTEGER NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO users (id, username, role) VALUES ('legacy_user', 'legacy', 'user')")
        .execute(&pool)
        .await
        .unwrap();
    let legacy_topic_id: i64 = sqlx::query_scalar(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ('legacy_user', 'legacy topic', 'repeatable_tip') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE user_documents (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT NOT NULL,
            topic_id INTEGER,
            source_type TEXT NOT NULL,
            title TEXT NOT NULL,
            url TEXT,
            content TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_documents (user_id, topic_id, source_type, title, content)
         VALUES ('legacy_user', ?, 'document', 'assigned', 'assigned content'),
                ('legacy_user', NULL, 'document', 'unassigned', 'unassigned content')",
    )
    .bind(legacy_topic_id)
    .execute(&pool)
    .await
    .unwrap();

    crate::apply_schema_migrations(&pool).await.unwrap();

    // New columns exist on topics.
    let has_col = |table: &str, col: &str| {
        let pool = pool.clone();
        let table = table.to_string();
        let col = col.to_string();
        async move {
            let row = sqlx::query("SELECT COUNT(*) AS c FROM pragma_table_info(?) WHERE name = ?")
                .bind(table)
                .bind(col)
                .fetch_one(&pool)
                .await
                .unwrap();
            row.get::<i64, _>("c") == 1
        }
    };
    assert!(has_col("topics", "grounding_strategy").await);
    assert!(has_col("topics", "image_strategy").await);
    assert!(has_col("tipcards", "use_image").await);
    assert!(has_col("tipcards", "image_query").await);
    assert!(has_col("user_settings", "grounding_strategy").await);
    assert!(has_col("user_settings", "llm_grounding_model").await);
    assert!(has_col("user_settings", "llm_grounding_reasoning_effort").await);
    assert!(has_col("user_settings", "image_sources").await);
    let image_defaults: (i64, String) = sqlx::query_as(
        "INSERT INTO tipcards (user_id, topic_id, full_content, compressed_content)
         VALUES ('legacy_user', ?, 'legacy card', 'legacy card')
         RETURNING use_image, image_query",
    )
    .bind(legacy_topic_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(image_defaults, (0, String::new()));
    let assignments: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'document_topics'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(assignments, 1);
    let backfilled: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM document_topics WHERE topic_id = ?")
            .bind(legacy_topic_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(backfilled, 1, "legacy topic assignment is backfilled");
    assert!(!has_col("user_documents", "topic_id").await);
    let unassigned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_topics WHERE document_id = (SELECT id FROM user_documents WHERE title = 'unassigned')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unassigned, 0, "legacy NULL assignment remains unassigned");

    // FTS5 virtual table is queryable (this is the FTS5 availability check).
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_chunks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

/// RAG retrieval is isolated to explicit topic assignments and supports reuse.
#[tokio::test]
async fn rag_retrieval_returns_matching_chunk() {
    let pool = setup_db().await;
    let user = "rag_user";
    sqlx::query("INSERT OR IGNORE INTO users (id, username, role) VALUES (?, 'rag', 'user')")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    // A topic owned by this user.
    let topic_id: i64 = sqlx::query_scalar(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES (?, 'borrow', 'repeatable_tip') RETURNING id",
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
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES (?, 'other', 'repeatable_tip') RETURNING id",
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
    sqlx::query("INSERT OR IGNORE INTO users (id, username, role) VALUES (?, 'pending', 'user')")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let topic_id: i64 = sqlx::query_scalar(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES (?, 'agg', 'repeatable_tip') RETURNING id",
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
    // SQLite CURRENT_TIMESTAMP has 1s resolution; nudge ordering via a backdated
    // created_at on the second card so the first is provably older.
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
    sqlx::query("UPDATE tipcards SET created_at = '2000-01-01 00:00:00' WHERE id = ?")
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE tipcards SET created_at = '2001-01-01 00:00:00' WHERE id = ?")
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
    let status: String = sqlx::query_scalar("SELECT status FROM review_states WHERE card_id = ?")
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
