use super::support::{
    TEST_USER_ID, bootstrap_api_key, make_state, post_api, setup_db, spawn_test_server,
    unique_settings_path,
};
use crate::apply_schema_migrations;
use prost::Message;
use sqlx::sqlite::SqlitePoolOptions;
use tokio::fs;

#[tokio::test]
async fn test_topic_names_can_repeat_across_users() {
    let db = setup_db().await;
    sqlx::query("INSERT INTO users (id, username, password_hash, role) VALUES (?, ?, ?, ?)")
        .bind("usr_other")
        .bind("other")
        .bind("")
        .bind("user")
        .execute(&db)
        .await
        .unwrap();

    crate::db::repositories::topics::get_or_create_topic(
        &db,
        TEST_USER_ID,
        "rust",
        "repeatable_tip",
        None,
    )
    .await
    .unwrap();
    crate::db::repositories::topics::get_or_create_topic(
        &db,
        "usr_other",
        "rust",
        "repeatable_tip",
        None,
    )
    .await
    .unwrap();

    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM topics WHERE name = 'rust'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn test_legacy_global_topic_unique_index_is_removed() {
    let db = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE topics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                tipcard_type TEXT NOT NULL DEFAULT 'repeatable_tip'
            )",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("CREATE TABLE tipcards (id INTEGER PRIMARY KEY AUTOINCREMENT, topic_id INTEGER NOT NULL, full_content TEXT NOT NULL, compressed_content TEXT NOT NULL)")
            .execute(&db)
            .await
            .unwrap();
    sqlx::query("CREATE TABLE review_states (id INTEGER PRIMARY KEY AUTOINCREMENT, card_id INTEGER NOT NULL UNIQUE, algorithm_used TEXT NOT NULL, state_data TEXT NOT NULL, next_review_at DATETIME NOT NULL)")
            .execute(&db)
            .await
            .unwrap();
    sqlx::query("CREATE TABLE api_keys (id INTEGER PRIMARY KEY AUTOINCREMENT, key_hash TEXT NOT NULL UNIQUE, client_name TEXT NOT NULL)")
            .execute(&db)
            .await
            .unwrap();
    apply_schema_migrations(&db).await.unwrap();
    sqlx::query("INSERT INTO users (id, username, password_hash, role) VALUES ('u1', 'u1', '', 'admin'), ('u2', 'u2', '', 'user')")
            .execute(&db)
            .await
            .unwrap();
    crate::db::repositories::topics::get_or_create_topic(&db, "u1", "rust", "repeatable_tip", None)
        .await
        .unwrap();
    crate::db::repositories::topics::get_or_create_topic(&db, "u2", "rust", "repeatable_tip", None)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_topic_daily_card_is_reused_after_review() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "daily_topic").await;

    let first_response = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Tips(
                crate::api::pb::TipsQuery {
                    count: 1,
                    topics: "rust".into(),
                    tipcard_type: "repeatable_tip".into(),
                    exclude_card_ids: vec![],
                    manual_content: "".into(),
                    manual_compressed_content: "".into(),
                },
            )),
        },
    )
    .await;
    assert_eq!(first_response.status(), reqwest::StatusCode::OK);
    let first = crate::api::pb::ApiResponse::decode(first_response.bytes().await.unwrap())
        .unwrap()
        .result
        .and_then(|result| match result {
            crate::api::pb::api_response::Result::Tips(tips) => tips.tips.first().cloned(),
            _ => None,
        })
        .expect("first tip");

    let review = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Review(
                crate::api::pb::ReviewPayload {
                    card_id: first.id,
                    grade: 4,
                    action: "".into(),
                },
            )),
        },
    )
    .await;
    assert_eq!(review.status(), reqwest::StatusCode::OK);

    let topics_response = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::ListAppTopics(
                crate::api::pb::Empty {},
            )),
        },
    )
    .await;
    assert_eq!(topics_response.status(), reqwest::StatusCode::OK);
    let topics = crate::api::pb::ApiResponse::decode(topics_response.bytes().await.unwrap())
        .unwrap()
        .result
        .and_then(|result| match result {
            crate::api::pb::api_response::Result::AppTopics(topics) => {
                topics.topics.into_iter().find(|topic| topic.name == "rust")
            }
            _ => None,
        })
        .expect("rust topic");

    let update_topic = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::UpdateTopic(
                crate::api::pb::UpdateTopicRequest {
                    id: topics.id,
                    prompt_template: Some("Give a smart tip about {topic}.".into()),
                    daily_card_count: Some(2),
                    daily_time_zone: Some("Asia/Vladivostok".into()),
                    daily_update_time: Some("06:30".into()),
                    compression_level: Some("strong".into()),
                },
            )),
        },
    )
    .await;
    assert_eq!(update_topic.status(), reqwest::StatusCode::OK);

    let second_response = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Tips(
                crate::api::pb::TipsQuery {
                    count: 1,
                    topics: "rust".into(),
                    tipcard_type: "repeatable_tip".into(),
                    exclude_card_ids: vec![],
                    manual_content: "".into(),
                    manual_compressed_content: "".into(),
                },
            )),
        },
    )
    .await;
    assert_eq!(second_response.status(), reqwest::StatusCode::OK);
    let second_tips =
        match crate::api::pb::ApiResponse::decode(second_response.bytes().await.unwrap())
            .unwrap()
            .result
            .unwrap()
        {
            crate::api::pb::api_response::Result::Tips(tips) => tips.tips,
            other => panic!("unexpected response: {:?}", other),
        };

    assert_eq!(second_tips.len(), 2);
    assert_eq!(second_tips[0].id, first.id);

    let topics_response = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key,
            op: Some(crate::api::pb::api_request::Op::ListAppTopics(
                crate::api::pb::Empty {},
            )),
        },
    )
    .await;
    let topics = crate::api::pb::ApiResponse::decode(topics_response.bytes().await.unwrap())
        .unwrap()
        .result
        .and_then(|result| match result {
            crate::api::pb::api_response::Result::AppTopics(topics) => {
                topics.topics.into_iter().find(|topic| topic.name == "rust")
            }
            _ => None,
        })
        .expect("updated rust topic");
    assert_eq!(topics.daily_card_count, 2);
    assert_eq!(topics.daily_time_zone, "Asia/Vladivostok");
    assert_eq!(topics.daily_update_time, "06:30");
    assert_eq!(topics.compression_level, "strong");
}

#[tokio::test]
async fn test_repeatable_tipcards_can_dismiss_and_get_new_card() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "repeatable_flow").await;

    let tips_query = crate::api::pb::TipsQuery {
        count: 1,
        topics: "spanish".into(),
        tipcard_type: "repeatable_tip".into(),
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
    assert_eq!(first_resp.tips[0].tipcard_type, "repeatable_tip");
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
            auth: api_key,
            op: Some(crate::api::pb::api_request::Op::Tips(tips_query)),
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
}

#[tokio::test]
async fn test_manual_tipcards_are_created_from_user_text() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "manual_flow").await;

    let tips_query = crate::api::pb::TipsQuery {
        count: 1,
        topics: "rust".into(),
        tipcard_type: "manual_tip".into(),
        exclude_card_ids: vec![],
        manual_content: "Borrow checker: one mutable borrow or many immutable borrows.".into(),
        manual_compressed_content: "".into(),
    };
    let res = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Tips(tips_query)),
        },
    )
    .await;
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let api_resp = crate::api::pb::ApiResponse::decode(res.bytes().await.unwrap()).unwrap();
    let tips_resp = match api_resp.result.unwrap() {
        crate::api::pb::api_response::Result::Tips(tips) => tips,
        other => panic!("unexpected response: {:?}", other),
    };
    assert_eq!(tips_resp.tips.len(), 1);
    assert_eq!(tips_resp.tips[0].tipcard_type, "manual_tip");
    assert_eq!(
        tips_resp.tips[0].full_content,
        "Borrow checker: one mutable borrow or many immutable borrows."
    );

    let ack = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key,
            op: Some(crate::api::pb::api_request::Op::Review(
                crate::api::pb::ReviewPayload {
                    card_id: tips_resp.tips[0].id,
                    grade: 3,
                    action: "acknowledge".into(),
                },
            )),
        },
    )
    .await;
    assert_eq!(ack.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn test_manual_tipcards_store_and_update_images() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);
    let image = "data:image/png;base64,iVBORw0KGgo=".to_string();

    let tips = crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(1),
            topics: "rust".into(),
            tipcard_type: Some("manual_tip".into()),
            exclude_card_ids: None,
            manual_content: Some("Manual card with image".into()),
            manual_compressed_content: None,
            manual_image_data: Some(vec![image.clone()]),
        },
    )
    .await
    .unwrap();

    assert_eq!(tips.len(), 1);
    assert_eq!(tips[0].image_data, vec![image.clone()]);

    let stored: String = sqlx::query_scalar("SELECT image_data FROM tipcards WHERE id = ?")
        .bind(tips[0].id)
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert!(
        serde_json::from_str::<Vec<String>>(&stored)
            .unwrap()
            .is_empty()
    );
    let stored_image: (String, i64) =
        sqlx::query_as("SELECT mime_type, byte_size FROM tipcard_images WHERE card_id = ?")
            .bind(tips[0].id)
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(stored_image.0, "image/png");
    assert!(stored_image.1 > 0);

    let replacement = "data:image/webp;base64,UklGRg==".to_string();
    crate::api::set_tipcard_images(&state, TEST_USER_ID, tips[0].id, vec![replacement.clone()])
        .await
        .unwrap();
    let updated: String = sqlx::query_scalar("SELECT image_data FROM tipcards WHERE id = ?")
        .bind(tips[0].id)
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert!(
        serde_json::from_str::<Vec<String>>(&updated)
            .unwrap()
            .is_empty()
    );
    let updated_image: (String, i64) =
        sqlx::query_as("SELECT mime_type, byte_size FROM tipcard_images WHERE card_id = ?")
            .bind(tips[0].id)
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(updated_image.0, "image/webp");
    assert!(updated_image.1 > 0);
}

#[tokio::test]
async fn test_list_images_for_cards_returns_stored_images() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);
    let image = "data:image/png;base64,iVBORw0KGgo=".to_string();

    let tips = crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(1),
            topics: "rust".into(),
            tipcard_type: Some("manual_tip".into()),
            exclude_card_ids: None,
            manual_content: Some("Manual card with image".into()),
            manual_compressed_content: None,
            manual_image_data: Some(vec![image]),
        },
    )
    .await
    .unwrap();

    let card_id = tips[0].id;
    let images = crate::db::repositories::tipcards::list_images_for_cards(
        &state.db,
        TEST_USER_ID,
        &[card_id],
    )
    .await
    .unwrap();

    assert_eq!(images.get(&card_id).map(|rows| rows.len()), Some(1));
}

#[tokio::test]
async fn test_pinned_tipcard_is_returned_before_schedule() {
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
    .bind("Pinned")
    .bind("Pinned full")
    .bind("Pinned compact")
    .execute(&state.db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query(
        "INSERT INTO review_states (card_id, algorithm_used, state_data, status, next_review_at) VALUES (?, ?, ?, 'active', ?)",
    )
    .bind(card_id)
    .bind("repeatable")
    .bind(r#"{"repeats":0}"#)
    .bind(chrono::Utc::now() + chrono::Duration::days(30))
    .execute(&state.db)
    .await
    .unwrap();

    crate::api::set_tipcard_pinned(&state, TEST_USER_ID, card_id, true)
        .await
        .unwrap();

    let cards = crate::api::build_tips(
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

    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].id, card_id);
    assert!(cards[0].pinned);
}

#[tokio::test]
async fn test_max_active_cards_blocks_new_manual_card_but_keeps_due_cards_available() {
    let settings_path = unique_settings_path();
    fs::write(
        &settings_path,
        "admin_token: test_admin_token_xyz\nmax_active_cards: 1\n",
    )
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
    .bind("Due")
    .bind("Due full")
    .bind("Due compact")
    .execute(&state.db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query(
        "INSERT INTO review_states (card_id, algorithm_used, state_data, status, next_review_at) VALUES (?, ?, ?, 'active', ?)",
    )
    .bind(card_id)
    .bind("repeatable")
    .bind(r#"{"repeats":0}"#)
    .bind(chrono::Utc::now())
    .execute(&state.db)
    .await
    .unwrap();

    let due_cards = crate::api::build_tips(
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
    assert_eq!(due_cards.len(), 1);
    assert_eq!(due_cards[0].id, card_id);

    let err = match crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(1),
            topics: "manual".into(),
            tipcard_type: Some("manual_tip".into()),
            exclude_card_ids: None,
            manual_content: Some("new manual".into()),
            manual_compressed_content: None,
            manual_image_data: None,
        },
    )
    .await
    {
        Ok(_) => panic!("manual card was created past max_active_cards"),
        Err(err) => err,
    };
    assert_eq!(err.0, axum::http::StatusCode::CONFLICT);
    assert_eq!(err.1, "Max active cards reached");
}

#[tokio::test]
async fn test_app_tip_replacement_excludes_visible_cards() {
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

    let mut visible_ids = Vec::new();
    for label in ["one", "two"] {
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
            "INSERT INTO review_states (card_id, algorithm_used, state_data, status, next_review_at) VALUES (?, ?, ?, 'active', ?)",
        )
        .bind(card_id)
        .bind("repeatable")
        .bind(r#"{"repeats":0}"#)
        .bind(chrono::Utc::now())
        .execute(&state.db)
        .await
        .unwrap();
        visible_ids.push(card_id);
    }

    crate::api::apply_review(&state, TEST_USER_ID, visible_ids[0], 3, "repeat")
        .await
        .unwrap();

    let replacement = crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(1),
            topics: "spanish".into(),
            tipcard_type: Some("repeatable_tip".into()),
            exclude_card_ids: Some(visible_ids.clone()),
            manual_content: None,
            manual_compressed_content: None,
            manual_image_data: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(replacement.len(), 1);
    assert!(
        !visible_ids.contains(&replacement[0].id),
        "replacement should not reuse a card already visible in the flow"
    );
}

#[tokio::test]
async fn test_custom_tipcards_do_not_create_review_state() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "custom_cards").await;

    let res = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::SubmitCustomTipcard(
                crate::api::pb::CustomTipcardRequest {
                    topic: "email summary".into(),
                    full_content: "Ship digest at 09:00.".into(),
                    compressed_content: "Digest 09:00".into(),
                    title: "Morning digest".into(),
                },
            )),
        },
    )
    .await;
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let api_resp = crate::api::pb::ApiResponse::decode(res.bytes().await.unwrap()).unwrap();
    let tips_resp = match api_resp.result.unwrap() {
        crate::api::pb::api_response::Result::Tips(tips) => tips,
        other => panic!("unexpected response: {:?}", other),
    };
    assert_eq!(tips_resp.tips.len(), 1);
    let card = &tips_resp.tips[0];
    assert_eq!(card.topic, "email summary");
    assert_eq!(card.tipcard_type, "custom_tip");
    assert_eq!(card.compressed_content, "Digest 09:00");

    let blocked_tips = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Tips(
                crate::api::pb::TipsQuery {
                    count: 1,
                    topics: "email summary".into(),
                    tipcard_type: "custom_tip".into(),
                    exclude_card_ids: vec![],
                    manual_content: "".into(),
                    manual_compressed_content: "".into(),
                },
            )),
        },
    )
    .await;
    assert_eq!(blocked_tips.status(), reqwest::StatusCode::BAD_REQUEST);

    let list = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::ListTipcards(
                crate::api::pb::Empty {},
            )),
        },
    )
    .await;
    assert_eq!(list.status(), reqwest::StatusCode::OK);
    let api_resp = crate::api::pb::ApiResponse::decode(list.bytes().await.unwrap()).unwrap();
    match api_resp.result.unwrap() {
        crate::api::pb::api_response::Result::Tipcards(cards) => {
            assert_eq!(cards.cards.len(), 1);
            assert_eq!(cards.cards[0].status, "custom");
        }
        other => panic!("unexpected response: {:?}", other),
    }

    let review = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Review(
                crate::api::pb::ReviewPayload {
                    card_id: card.id,
                    grade: 3,
                    action: "acknowledge".into(),
                },
            )),
        },
    )
    .await;
    assert_eq!(review.status(), reqwest::StatusCode::NOT_FOUND);

    let summary = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key,
            op: Some(crate::api::pb::api_request::Op::GetSummary(
                crate::api::pb::Empty {},
            )),
        },
    )
    .await;
    assert_eq!(summary.status(), reqwest::StatusCode::OK);
    let api_resp = crate::api::pb::ApiResponse::decode(summary.bytes().await.unwrap()).unwrap();
    match api_resp.result.unwrap() {
        crate::api::pb::api_response::Result::Summary(summary) => {
            assert_eq!(summary.total_cards, 1);
            assert_eq!(summary.active_cards, 0);
            assert_eq!(summary.due_cards, 0);
        }
        other => panic!("unexpected response: {:?}", other),
    }
}
