use super::support::{
    TEST_USER_ID, bootstrap_api_key, make_state, post_api, setup_db, spawn_test_server,
    unique_settings_path,
};
use prost::Message;
use std::sync::Arc;
use tokio::fs;

#[tokio::test]
async fn test_force_daily_refresh_respects_pending_low_water_mark() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "force_daily_refresh").await;

    let tips_query = crate::api::pb::TipsQuery {
        count: 1,
        topics: "rust".into(),
        tipcard_type: "repeatable_tip".into(),
        exclude_card_ids: vec![],
        manual_content: "".into(),
        manual_compressed_content: "".into(),
    };
    let first_response = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Tips(tips_query.clone())),
        },
    )
    .await;
    assert_eq!(first_response.status(), reqwest::StatusCode::OK);
    let first_id = match crate::api::pb::ApiResponse::decode(first_response.bytes().await.unwrap())
        .unwrap()
        .result
        .unwrap()
    {
        crate::api::pb::api_response::Result::Tips(tips) => tips.tips[0].id,
        other => panic!("unexpected response: {:?}", other),
    };

    let refresh_response = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::ForceDailyRefresh(
                crate::api::pb::ForceDailyRefreshRequest {
                    topics: "".into(),
                    tipcard_type: "".into(),
                },
            )),
        },
    )
    .await;
    assert_eq!(refresh_response.status(), reqwest::StatusCode::OK);
    let refresh_result =
        match crate::api::pb::ApiResponse::decode(refresh_response.bytes().await.unwrap())
            .unwrap()
            .result
            .unwrap()
        {
            crate::api::pb::api_response::Result::ForceDailyRefresh(result) => result,
            other => panic!("unexpected response: {:?}", other),
        };
    assert_eq!(refresh_result.refreshed_cards, 0);
    assert_eq!(refresh_result.available_cards, 0);
    assert_eq!(
        refresh_result.outcome,
        crate::api::pb::ForceDailyRefreshOutcome::NoChange as i32
    );

    let second_response = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Tips(tips_query.clone())),
        },
    )
    .await;
    assert_eq!(second_response.status(), reqwest::StatusCode::OK);
    let second_id =
        match crate::api::pb::ApiResponse::decode(second_response.bytes().await.unwrap())
            .unwrap()
            .result
            .unwrap()
        {
            crate::api::pb::api_response::Result::Tips(tips) => tips.tips[0].id,
            other => panic!("unexpected response: {:?}", other),
        };
    assert_eq!(second_id, first_id);

    let excluded_response = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Tips(
                crate::api::pb::TipsQuery {
                    exclude_card_ids: vec![first_id],
                    ..tips_query
                },
            )),
        },
    )
    .await;
    assert_eq!(excluded_response.status(), reqwest::StatusCode::OK);
    let new_id = match crate::api::pb::ApiResponse::decode(excluded_response.bytes().await.unwrap())
        .unwrap()
        .result
        .unwrap()
    {
        crate::api::pb::api_response::Result::Tips(tips) => tips.tips[0].id,
        other => panic!("unexpected response: {:?}", other),
    };
    assert_ne!(new_id, first_id);

    let cards = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key,
            op: Some(crate::api::pb::api_request::Op::ListTipcards(
                crate::api::pb::Empty {},
            )),
        },
    )
    .await;
    assert_eq!(cards.status(), reqwest::StatusCode::OK);
    let listed_cards = match crate::api::pb::ApiResponse::decode(cards.bytes().await.unwrap())
        .unwrap()
        .result
        .unwrap()
    {
        crate::api::pb::api_response::Result::Tipcards(cards) => cards.cards,
        other => panic!("unexpected response: {:?}", other),
    };
    let first_status = listed_cards
        .iter()
        .find(|card| card.id == first_id)
        .map(|card| card.status.as_str())
        .expect("first card remains listed");
    assert_eq!(first_status, "active");
    assert!(
        listed_cards.iter().any(|card| card.id == new_id),
        "new card should be listed alongside the first card"
    );
}

#[tokio::test]
async fn explicit_repeatable_load_rotates_pending_card_at_active_limit() {
    let settings_path = unique_settings_path();
    fs::write(
        &settings_path,
        "admin_token: test_admin_token_xyz\nmax_active_cards: 1\n",
    )
    .await
    .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);
    let topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ($1, 'loaded deck', 'repeatable_tip') RETURNING id",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&state.db)
    .await
    .unwrap();
    let current_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "Current",
        "Current full",
        "Current compact",
        false,
        "",
        "active",
    )
    .await
    .unwrap();
    let pending_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "Pending replacement",
        "Pending replacement full",
        "Pending replacement compact",
        false,
        "",
        "pending",
    )
    .await
    .unwrap();

    let result = crate::api::tips::force_daily_refresh(
        &state,
        TEST_USER_ID,
        crate::api::ForceDailyRefreshRequest {
            topics: "loaded deck".into(),
            tipcard_type: Some("repeatable_tip".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(result.refreshed_cards, 1);
    assert_eq!(result.available_cards, 1);
    assert_eq!(result.generated_cards, 0);
    assert_eq!(
        result.outcome,
        crate::types::ForceDailyRefreshOutcome::CardAvailable
    );

    let current_status: String =
        sqlx::query_scalar("SELECT status FROM review_states WHERE card_id = $1")
            .bind(current_id)
            .fetch_one(&state.db)
            .await
            .unwrap();
    let pending_status: String =
        sqlx::query_scalar("SELECT status FROM review_states WHERE card_id = $1")
            .bind(pending_id)
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(current_status, "pending");
    assert_eq!(pending_status, "active");
    assert_eq!(
        crate::db::repositories::tipcards::active_card_count(&state.db, TEST_USER_ID)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn explicit_repeatable_load_generates_and_promotes_when_queue_is_empty() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);
    let topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type)
         VALUES ($1, 'empty loaded deck', 'repeatable_tip') RETURNING id",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&state.db)
    .await
    .unwrap();

    let result = crate::api::tips::force_daily_refresh(
        &state,
        TEST_USER_ID,
        crate::api::ForceDailyRefreshRequest {
            topics: "empty loaded deck".into(),
            tipcard_type: Some("repeatable_tip".into()),
        },
    )
    .await
    .unwrap();

    assert_eq!(result.refreshed_cards, 1);
    assert_eq!(result.available_cards, 1);
    assert!(result.generated_cards >= 5);
    assert_eq!(
        result.outcome,
        crate::types::ForceDailyRefreshOutcome::CardAvailable
    );
    let active = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         WHERE t.topic_id = $1 AND r.status = 'active' AND r.next_review_at <= NOW()",
    )
    .bind(topic_id)
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(active, 1);
    let pending = crate::db::repositories::tipcards::count_pending(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
    )
    .await
    .unwrap();
    assert_eq!(pending as u64, result.generated_cards - 1);
}

#[tokio::test]
async fn explicit_load_reports_active_limit_when_no_topic_slot_exists() {
    let settings_path = unique_settings_path();
    fs::write(
        &settings_path,
        "admin_token: test_admin_token_xyz\nmax_active_cards: 1\n",
    )
    .await
    .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);
    let occupied_topic = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type)
         VALUES ($1, 'occupied', 'casual_tip') RETURNING id",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&state.db)
    .await
    .unwrap();
    crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        occupied_topic,
        "casual_tip",
        "Occupied",
        "occupied full",
        "occupied compact",
        false,
        "",
        "active",
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO topics (user_id, name, tipcard_type)
         VALUES ($1, 'blocked deck', 'repeatable_tip')",
    )
    .bind(TEST_USER_ID)
    .execute(&state.db)
    .await
    .unwrap();

    let result = crate::api::tips::force_daily_refresh(
        &state,
        TEST_USER_ID,
        crate::api::ForceDailyRefreshRequest {
            topics: "blocked deck".into(),
            tipcard_type: Some("repeatable_tip".into()),
        },
    )
    .await
    .unwrap();

    assert_eq!(result.refreshed_cards, 0);
    assert_eq!(result.available_cards, 0);
    assert_eq!(result.generated_cards, 0);
    assert_eq!(
        result.outcome,
        crate::types::ForceDailyRefreshOutcome::ActiveLimitReached
    );
}

#[tokio::test]
async fn explicit_casual_load_reports_existing_pending_as_available_not_generated() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);
    let topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type)
         VALUES ($1, 'casual queue', 'casual_tip') RETURNING id",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&state.db)
    .await
    .unwrap();
    crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "casual_tip",
        "Queued casual",
        "queued full",
        "queued compact",
        false,
        "",
        "pending",
    )
    .await
    .unwrap();

    let result = crate::api::tips::force_daily_refresh(
        &state,
        TEST_USER_ID,
        crate::api::ForceDailyRefreshRequest {
            topics: "casual queue".into(),
            tipcard_type: Some("casual_tip".into()),
        },
    )
    .await
    .unwrap();

    assert_eq!(result.refreshed_cards, 1);
    assert_eq!(result.available_cards, 1);
    assert_eq!(result.generated_cards, 0);
    assert_eq!(
        result.outcome,
        crate::types::ForceDailyRefreshOutcome::CardAvailable
    );
}

#[tokio::test]
async fn test_daily_refresh_keeps_current_card_and_exclude_promotes_pending_card() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "{}").await.unwrap();
    let db = setup_db().await;
    let state = Arc::new(make_state(db, settings_path));

    let request = crate::api::TipsJsonRequest {
        count: Some(1),
        topics: "rust".into(),
        tipcard_type: Some("repeatable_tip".into()),
        exclude_card_ids: None,
        manual_content: None,
        manual_compressed_content: None,
        manual_image_data: None,
    };

    let first = crate::api::build_tips(&state, TEST_USER_ID, request.clone())
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    let first_id = first[0].id;

    sqlx::query("UPDATE tipcards SET created_at = '2000-01-01 00:00:00' WHERE id = $1")
        .bind(first_id)
        .execute(&state.db)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE review_states SET next_review_at = '2999-01-01 00:00:00' WHERE card_id = $1",
    )
    .bind(first_id)
    .execute(&state.db)
    .await
    .unwrap();

    let automatic_refresh = crate::api::build_tips(&state, TEST_USER_ID, request.clone())
        .await
        .unwrap();
    assert_eq!(automatic_refresh.len(), 1);
    assert_eq!(automatic_refresh[0].id, first_id);
    let card_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tipcards")
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(card_count, 5);

    // Browser settings sends an empty topic list and JSON null card type.
    let forced = crate::api::tips::force_daily_refresh(
        &state,
        TEST_USER_ID,
        crate::api::ForceDailyRefreshRequest {
            topics: "".into(),
            tipcard_type: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(forced.refreshed_cards, 0);
    assert_eq!(
        forced.outcome,
        crate::types::ForceDailyRefreshOutcome::NoChange
    );

    let after_force = crate::api::build_tips(&state, TEST_USER_ID, request.clone())
        .await
        .unwrap();
    assert_eq!(after_force.len(), 1);
    assert_eq!(after_force[0].id, first_id);

    let fresh = crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            exclude_card_ids: Some(vec![first_id]),
            ..request
        },
    )
    .await
    .unwrap();
    assert_eq!(fresh.len(), 1);
    assert_ne!(fresh[0].id, first_id);
    let card_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tipcards")
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(card_count, 5);
}

#[tokio::test]
async fn test_initial_card_counts_as_current_daily_refresh_window() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "{}").await.unwrap();
    let db = setup_db().await;
    let state = Arc::new(make_state(db, settings_path));

    crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(1),
            topics: "rust".into(),
            tipcard_type: Some("repeatable_tip".into()),
            exclude_card_ids: None,
            manual_content: None,
            manual_compressed_content: None,
            manual_image_data: None,
        },
    )
    .await
    .unwrap();

    let first_refresh = crate::api::refresh_due_daily_topics(&state).await.unwrap();
    assert_eq!(first_refresh, 0);

    let second_refresh = crate::api::refresh_due_daily_topics(&state).await.unwrap();
    assert_eq!(second_refresh, 0);

    let card_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tipcards")
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(card_count, 5);
}

#[tokio::test]
async fn test_repeatable_review_uses_srs_schedule() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);

    let topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type, daily_card_count)
         VALUES ('usr_test_admin', $1, $2, $3) RETURNING id",
    )
    .bind("spanish")
    .bind("repeatable_tip")
    .bind(2_i64)
    .fetch_one(&state.db)
    .await
    .unwrap();
    let card_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content) VALUES ('usr_test_admin', $1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(topic_id)
    .bind("repeatable_tip")
    .bind("known")
    .bind("Full known")
    .bind("Compressed known")
    .fetch_one(&state.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO review_states (card_id, algorithm_used, state_data, status, next_review_at) VALUES ($1, $2, $3, 'active', $4)",
    )
    .bind(card_id)
    .bind("sm2")
    .bind(r#"{"repeats":0}"#)
    .bind(chrono::Utc::now())
    .execute(&state.db)
    .await
    .unwrap();

    let stacked_pending_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "stacked next card",
        "stacked full content",
        "stacked compact content",
        false,
        "",
        "pending",
    )
    .await
    .unwrap();

    let before = chrono::Utc::now();
    crate::api::apply_review(&state, TEST_USER_ID, card_id, 1, "again")
        .await
        .unwrap();

    let (status, state_data, feedback, reviewed_at, next_review_at) = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<chrono::DateTime<chrono::Utc>>,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
            "SELECT status, state_data, feedback, reviewed_at, next_review_at FROM review_states WHERE card_id = $1",
        )
        .bind(card_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
    let state_json: serde_json::Value = serde_json::from_str(&state_data).unwrap();
    assert_eq!(status, "active");
    assert_eq!(state_json["repeats"], 1);
    assert_eq!(state_json["scheduling_state"]["repetitions"], 0);
    assert_eq!(state_json["scheduling_state"]["interval"], 1);
    assert_eq!(feedback, "again");
    assert!(reviewed_at.is_some());
    assert!(next_review_at > before);
    let stacked_status: String =
        sqlx::query_scalar("SELECT status FROM review_states WHERE card_id = $1")
            .bind(stacked_pending_id)
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(stacked_status, "pending");
    let promoted = crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(1),
            topics: "spanish".into(),
            tipcard_type: Some("repeatable_tip".into()),
            exclude_card_ids: Some(vec![card_id]),
            manual_content: None,
            manual_compressed_content: None,
            manual_image_data: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(promoted.len(), 1);
    assert_eq!(promoted[0].id, stacked_pending_id);

    let stale_pending_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "stale next card",
        "stale full content",
        "stale compact content",
        false,
        "",
        "pending",
    )
    .await
    .unwrap();
    let stale_active_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "stale unseen card",
        "stale unseen full content",
        "stale unseen compact content",
        false,
        "",
        "active",
    )
    .await
    .unwrap();

    crate::api::apply_review(&state, TEST_USER_ID, card_id, 1, "skip_too_difficult")
        .await
        .unwrap();
    let (status, feedback) = sqlx::query_as::<_, (String, String)>(
        "SELECT status, feedback FROM review_states WHERE card_id = $1",
    )
    .bind(card_id)
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(status, "dismissed");
    assert_eq!(feedback, "too_difficult");
    let (status, feedback) = sqlx::query_as::<_, (String, String)>(
        "SELECT status, feedback FROM review_states WHERE card_id = $1",
    )
    .bind(stale_pending_id)
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(status, "dismissed");
    assert_eq!(feedback, "superseded");
    let (status, feedback) = sqlx::query_as::<_, (String, String)>(
        "SELECT status, feedback FROM review_states WHERE card_id = $1",
    )
    .bind(stale_active_id)
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(status, "dismissed");
    assert_eq!(feedback, "superseded");
    assert_eq!(
        crate::db::repositories::tipcards::active_card_count(&state.db, TEST_USER_ID)
            .await
            .unwrap(),
        0
    );

    let fresh_active_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "fresh personalized card",
        "fresh personalized full content",
        "fresh personalized compact content",
        false,
        "",
        "active",
    )
    .await
    .unwrap();
    let due = crate::db::repositories::tipcards::find_due_topic_cards(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        &[],
        10,
    )
    .await
    .unwrap();
    assert!(due.iter().any(|card| card.id == fresh_active_id));

    let fresh_pending_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "fresh pending card",
        "fresh pending full content",
        "fresh pending compact content",
        false,
        "",
        "pending",
    )
    .await
    .unwrap();
    let promoted = crate::db::repositories::tipcards::take_pending_card(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
    )
    .await
    .unwrap()
    .expect("post-feedback pending card remains eligible");
    assert_eq!(promoted.id, fresh_pending_id);
}

#[tokio::test]
async fn concurrent_reviews_serialize_scheduling_state_updates() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);

    let topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type)
         VALUES ($1, 'concurrent reviews', 'repeatable_tip') RETURNING id",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&state.db)
    .await
    .unwrap();
    let card_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "serialized review",
        "full",
        "compact",
        false,
        "",
        "active",
    )
    .await
    .unwrap();

    let barrier = Arc::new(tokio::sync::Barrier::new(3));
    let first_service = state.reviews.clone();
    let first_barrier = barrier.clone();
    let first = tokio::spawn(async move {
        first_barrier.wait().await;
        first_service
            .apply_review(TEST_USER_ID, card_id, 5, "learned")
            .await
    });
    let second_service = state.reviews.clone();
    let second_barrier = barrier.clone();
    let second = tokio::spawn(async move {
        second_barrier.wait().await;
        second_service
            .apply_review(TEST_USER_ID, card_id, 5, "learned")
            .await
    });
    barrier.wait().await;
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();

    let (repeats, state_data) = sqlx::query_as::<_, (i64, String)>(
        "SELECT repeats, state_data FROM review_states WHERE card_id = $1",
    )
    .bind(card_id)
    .fetch_one(&state.db)
    .await
    .unwrap();
    let state: crate::domain::review::RepeatableState = serde_json::from_str(&state_data).unwrap();
    assert_eq!(repeats, 2);
    assert_eq!(state.repeats, 2);
    assert_eq!(state.scheduling_state.data.repetitions, 2);
    assert_eq!(state.scheduling_state.data.interval, 6);
}

#[tokio::test]
async fn test_casual_acknowledge_uses_srs_schedule() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);

    let topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ('usr_test_admin', $1, $2) RETURNING id",
    )
    .bind("rust")
    .bind("casual_tip")
    .fetch_one(&state.db)
    .await
    .unwrap();
    let card_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content) VALUES ('usr_test_admin', $1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(topic_id)
    .bind("casual_tip")
    .bind("known")
    .bind("Full known")
    .bind("Compressed known")
    .fetch_one(&state.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO review_states (card_id, algorithm_used, state_data, status, next_review_at) VALUES ($1, $2, $3, 'active', $4)",
    )
    .bind(card_id)
    .bind("sm2")
    .bind(r#"{"repeats":0}"#)
    .bind(chrono::Utc::now())
    .execute(&state.db)
    .await
    .unwrap();

    let before = chrono::Utc::now();
    crate::api::apply_review(&state, TEST_USER_ID, card_id, 3, "acknowledge")
        .await
        .unwrap();

    let (status, state_data, next_review_at) =
        sqlx::query_as::<_, (String, String, chrono::DateTime<chrono::Utc>)>(
            "SELECT status, state_data, next_review_at FROM review_states WHERE card_id = $1",
        )
        .bind(card_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
    let state_json: serde_json::Value = serde_json::from_str(&state_data).unwrap();
    assert_eq!(status, "active");
    assert_eq!(state_json["scheduling_state"]["repetitions"], 1);
    assert_eq!(state_json["scheduling_state"]["interval"], 1);
    assert!(next_review_at > before);
}

#[tokio::test]
async fn test_repeatable_due_selection_prefers_known_cards() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);

    let topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type, daily_card_count)
         VALUES ('usr_test_admin', $1, $2, $3) RETURNING id",
    )
    .bind("spanish")
    .bind("repeatable_tip")
    .bind(2_i64)
    .fetch_one(&state.db)
    .await
    .unwrap();

    let now = chrono::Utc::now();
    let mut card_ids = Vec::new();
    for (label, repeats, due_at) in [
        ("new one", 0_u32, now - chrono::Duration::minutes(30)),
        ("known", 2_u32, now - chrono::Duration::minutes(5)),
        ("new two", 0_u32, now - chrono::Duration::minutes(20)),
        ("new three", 0_u32, now - chrono::Duration::minutes(15)),
        ("new four", 0_u32, now - chrono::Duration::minutes(10)),
    ] {
        let card_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content) VALUES ('usr_test_admin', $1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(topic_id)
        .bind("repeatable_tip")
        .bind(label)
        .bind(format!("Full {label}"))
        .bind(format!("Compressed {label}"))
        .fetch_one(&state.db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO review_states (card_id, algorithm_used, state_data, repeats, status, next_review_at) VALUES ($1, $2, $3, $4, 'active', $5)",
        )
        .bind(card_id)
        .bind("repeatable")
        .bind(format!(r#"{{"repeats":{repeats}}}"#))
        .bind(i64::from(repeats))
        .bind(due_at)
        .execute(&state.db)
        .await
        .unwrap();
        card_ids.push(card_id);
    }

    let tips = crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(1),
            topics: "spanish".into(),
            tipcard_type: Some("repeatable_tip".into()),
            exclude_card_ids: None,
            manual_content: None,
            manual_compressed_content: None,
            manual_image_data: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(tips.len(), 1);
    assert_eq!(tips[0].id, card_ids[1]);
    assert_eq!(
        crate::db::repositories::tipcards::count_pending(
            &state.db,
            TEST_USER_ID,
            topic_id,
            "repeatable_tip",
        )
        .await
        .unwrap(),
        4
    );

    let flow_cards =
        crate::db::repositories::tipcards::list_flow_cards(&state.db, TEST_USER_ID, None, 10)
            .await
            .unwrap();
    assert_eq!(flow_cards.len(), 1);
    assert_eq!(flow_cards[0].id, card_ids[1]);

    crate::api::apply_review(&state, TEST_USER_ID, card_ids[1], 3, "skip_not_interested")
        .await
        .unwrap();
    let replacement = crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(1),
            topics: "spanish".into(),
            tipcard_type: Some("repeatable_tip".into()),
            exclude_card_ids: Some(vec![card_ids[1]]),
            manual_content: None,
            manual_compressed_content: None,
            manual_image_data: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(replacement.len(), 1);
    assert_ne!(replacement[0].id, card_ids[1]);
}

#[tokio::test]
async fn test_repeatable_load_creates_one_active_card_and_pending_deck() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);

    let initial = crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(1),
            topics: "japanese".into(),
            tipcard_type: Some("repeatable_tip".into()),
            exclude_card_ids: None,
            manual_content: None,
            manual_compressed_content: None,
            manual_image_data: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(initial.len(), 1);

    let cards = crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(5),
            topics: "japanese".into(),
            tipcard_type: Some("repeatable_tip".into()),
            exclude_card_ids: None,
            manual_content: None,
            manual_compressed_content: None,
            manual_image_data: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].id, initial[0].id);
    let (active, pending) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT
            SUM(CASE WHEN r.status = 'active' THEN 1 ELSE 0 END),
            SUM(CASE WHEN r.status = 'pending' THEN 1 ELSE 0 END)
         FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         WHERE t.user_id = $1 AND t.tipcard_type = 'repeatable_tip'",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(active, 1);
    assert_eq!(pending, 4);
}

#[tokio::test]
async fn test_casual_tipcards_can_dismiss_or_acknowledge_and_get_new_card() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "casual_flow").await;

    let tips_query = crate::api::pb::TipsQuery {
        count: 1,
        topics: "rust".into(),
        tipcard_type: "casual_tip".into(),
        exclude_card_ids: vec![],
        manual_content: "".into(),
        manual_compressed_content: "".into(),
    };
    let res = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Tips(tips_query.clone())),
        },
    )
    .await;
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let api_resp = crate::api::pb::ApiResponse::decode(res.bytes().await.unwrap()).unwrap();
    let first_resp = match api_resp.result.unwrap() {
        crate::api::pb::api_response::Result::Tips(tips) => tips,
        other => panic!("unexpected response: {:?}", other),
    };
    assert_eq!(first_resp.tips.len(), 1);
    assert_eq!(first_resp.tips[0].tipcard_type, "casual_tip");
    let first_id = first_resp.tips[0].id;

    let dismiss = crate::api::pb::ReviewPayload {
        card_id: first_id,
        grade: 0,
        action: "dismiss".into(),
    };
    let res = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Review(dismiss)),
        },
    )
    .await;
    assert_eq!(res.status(), reqwest::StatusCode::OK);

    let res = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Tips(tips_query.clone())),
        },
    )
    .await;
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let api_resp = crate::api::pb::ApiResponse::decode(res.bytes().await.unwrap()).unwrap();
    let second_resp = match api_resp.result.unwrap() {
        crate::api::pb::api_response::Result::Tips(tips) => tips,
        other => panic!("unexpected response: {:?}", other),
    };
    assert_eq!(second_resp.tips.len(), 1);
    assert_ne!(second_resp.tips[0].id, first_id);
    let second_id = second_resp.tips[0].id;

    let acknowledge = crate::api::pb::ReviewPayload {
        card_id: second_id,
        grade: 5,
        action: "acknowledge".into(),
    };
    let res = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Review(acknowledge)),
        },
    )
    .await;
    assert_eq!(res.status(), reqwest::StatusCode::OK);

    let res = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key,
            op: Some(crate::api::pb::api_request::Op::Tips(tips_query)),
        },
    )
    .await;
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let api_resp = crate::api::pb::ApiResponse::decode(res.bytes().await.unwrap()).unwrap();
    let third_resp = match api_resp.result.unwrap() {
        crate::api::pb::api_response::Result::Tips(tips) => tips,
        other => panic!("unexpected response: {:?}", other),
    };
    assert_eq!(third_resp.tips.len(), 1);
    assert_ne!(third_resp.tips[0].id, second_id);
}

#[tokio::test]
async fn review_and_advance_returns_a_due_sibling_when_pending_is_empty() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);

    let topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type, daily_card_count)
         VALUES ($1, 'sibling slot', 'repeatable_tip', 5) RETURNING id",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&state.db)
    .await
    .unwrap();
    let reviewed_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "First due",
        "full",
        "compact",
        false,
        "",
        "active",
    )
    .await
    .unwrap();
    let sibling_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "Second due",
        "full",
        "compact",
        false,
        "",
        "active",
    )
    .await
    .unwrap();
    crate::db::repositories::tipcards::set_pinned(&state.db, TEST_USER_ID, reviewed_id, true)
        .await
        .unwrap();

    let result = state
        .reviews
        .apply_review_and_advance(
            TEST_USER_ID,
            reviewed_id,
            5,
            "learned",
            crate::services::review::ReviewAdvancePolicy {
                window_start: chrono::Utc::now() - chrono::Duration::days(1),
                daily_limit: 5,
            },
        )
        .await
        .unwrap();

    assert_eq!(result.next_card_id, Some(sibling_id));
    assert!(!result.daily_complete);
    let sibling_pinned: i64 = sqlx::query_scalar("SELECT pinned FROM tipcards WHERE id = $1")
        .bind(sibling_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(sibling_pinned, 1);
}
