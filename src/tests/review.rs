use super::support::{
    TEST_USER_ID, bootstrap_api_key, make_state, post_api, setup_db, spawn_test_server,
    unique_settings_path,
};
use prost::Message;
use std::sync::Arc;
use tokio::fs;

#[tokio::test]
async fn test_force_daily_refresh_keeps_current_daily_card_and_allows_new_card() {
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
    let refreshed_cards =
        match crate::api::pb::ApiResponse::decode(refresh_response.bytes().await.unwrap())
            .unwrap()
            .result
            .unwrap()
        {
            crate::api::pb::api_response::Result::ForceDailyRefresh(result) => {
                result.refreshed_cards
            }
            other => panic!("unexpected response: {:?}", other),
        };
    assert_eq!(refreshed_cards, 1);

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
async fn test_daily_refresh_keeps_current_unpinned_daily_card_and_exclude_adds_new_card() {
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

    sqlx::query("UPDATE tipcards SET created_at = '2000-01-01 00:00:00' WHERE id = ?")
        .bind(first_id)
        .execute(&state.db)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE review_states SET next_review_at = '2999-01-01 00:00:00' WHERE card_id = ?",
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
    assert_eq!(card_count, 1);

    let forced = crate::api::tips::force_daily_refresh(
        &state,
        TEST_USER_ID,
        crate::api::ForceDailyRefreshRequest {
            topics: "rust".into(),
            tipcard_type: Some("repeatable_tip".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(forced.refreshed_cards, 1);

    let after_force = crate::api::build_tips(&state, TEST_USER_ID, request.clone())
        .await
        .unwrap();
    assert_eq!(after_force.len(), 1);
    assert_ne!(after_force[0].id, first_id);

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
    assert_eq!(fresh[0].id, after_force[0].id);
    let card_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tipcards")
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(card_count, 2);
}

#[tokio::test]
async fn test_server_daily_refresh_runs_once_per_window() {
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
    assert_eq!(first_refresh, 1);

    let second_refresh = crate::api::refresh_due_daily_topics(&state).await.unwrap();
    assert_eq!(second_refresh, 0);

    let card_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tipcards")
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(card_count, 2);
}

#[tokio::test]
async fn test_repeatable_review_uses_srs_schedule() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);

    let topic_id = sqlx::query(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ('usr_test_admin', ?, ?)",
    )
    .bind("spanish")
    .bind("repeatable_tip")
    .execute(&state.db)
    .await
    .unwrap()
    .last_insert_rowid();
    let card_id = sqlx::query(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content) VALUES ('usr_test_admin', ?, ?, ?, ?, ?)",
    )
    .bind(topic_id)
    .bind("repeatable_tip")
    .bind("known")
    .bind("Full known")
    .bind("Compressed known")
    .execute(&state.db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query(
        "INSERT INTO review_states (card_id, algorithm_used, state_data, status, next_review_at) VALUES (?, ?, ?, 'active', ?)",
    )
    .bind(card_id)
    .bind("sm2")
    .bind(r#"{"repeats":0}"#)
    .bind(chrono::Utc::now())
    .execute(&state.db)
    .await
    .unwrap();

    let before = chrono::Utc::now();
    crate::api::apply_review(&state, TEST_USER_ID, card_id, 1, "repeat")
        .await
        .unwrap();

    let (status, state_data, next_review_at) =
        sqlx::query_as::<_, (String, String, chrono::DateTime<chrono::Utc>)>(
            "SELECT status, state_data, next_review_at FROM review_states WHERE card_id = ?",
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
    assert!(next_review_at > before);
}

#[tokio::test]
async fn test_casual_acknowledge_uses_srs_schedule() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);

    let topic_id = sqlx::query(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ('usr_test_admin', ?, ?)",
    )
    .bind("rust")
    .bind("casual_tip")
    .execute(&state.db)
    .await
    .unwrap()
    .last_insert_rowid();
    let card_id = sqlx::query(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content) VALUES ('usr_test_admin', ?, ?, ?, ?, ?)",
    )
    .bind(topic_id)
    .bind("casual_tip")
    .bind("known")
    .bind("Full known")
    .bind("Compressed known")
    .execute(&state.db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query(
        "INSERT INTO review_states (card_id, algorithm_used, state_data, status, next_review_at) VALUES (?, ?, ?, 'active', ?)",
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
            "SELECT status, state_data, next_review_at FROM review_states WHERE card_id = ?",
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

    let topic_id = sqlx::query(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ('usr_test_admin', ?, ?)",
    )
    .bind("spanish")
    .bind("repeatable_tip")
    .execute(&state.db)
    .await
    .unwrap()
    .last_insert_rowid();

    let now = chrono::Utc::now();
    let mut card_ids = Vec::new();
    for (label, repeats, due_at) in [
        ("new", 0_u32, now - chrono::Duration::minutes(30)),
        ("known", 2_u32, now - chrono::Duration::minutes(5)),
    ] {
        let card_id = sqlx::query(
            "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content) VALUES ('usr_test_admin', ?, ?, ?, ?, ?)",
        )
        .bind(topic_id)
        .bind("repeatable_tip")
        .bind(label)
        .bind(format!("Full {label}"))
        .bind(format!("Compressed {label}"))
        .execute(&state.db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query(
            "INSERT INTO review_states (card_id, algorithm_used, state_data, repeats, status, next_review_at) VALUES (?, ?, ?, ?, 'active', ?)",
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
