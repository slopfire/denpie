use super::support::{
    TEST_USER_ID, bootstrap_api_key, make_state, post_api, setup_db, spawn_test_server,
    unique_settings_path,
};
use prost::Message;
use tokio::fs;

#[tokio::test]
async fn test_topic_names_can_repeat_across_users() {
    let db = setup_db().await;
    sqlx::query("INSERT INTO users (id, username, password_hash, role) VALUES ($1, $2, $3, $4)")
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
async fn card_creation_rejects_foreign_and_mismatched_topics() {
    let db = setup_db().await;
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role) VALUES ($1, $2, '', 'user')",
    )
    .bind("usr_card_owner")
    .bind("card-owner")
    .execute(&db)
    .await
    .unwrap();
    let foreign_topic = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type)
         VALUES ($1, 'foreign cards', 'repeatable_tip') RETURNING id",
    )
    .bind("usr_card_owner")
    .fetch_one(&db)
    .await
    .unwrap();

    let foreign_result = crate::db::repositories::tipcards::create_generated_with_status(
        &db,
        TEST_USER_ID,
        foreign_topic,
        "repeatable_tip",
        "foreign",
        "full",
        "compact",
        false,
        "",
        "pending",
    )
    .await;
    assert!(matches!(
        foreign_result,
        Err(crate::error::AppError::NotFound(_))
    ));

    let own_topic = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type)
         VALUES ($1, 'typed cards', 'repeatable_tip') RETURNING id",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&db)
    .await
    .unwrap();
    let cards = [crate::db::repositories::tipcards::GeneratedCardParams {
        title: "wrong type",
        full_content: "full",
        compressed_content: "compact",
        use_image: false,
        image_query: "",
    }];
    let mismatched_result = crate::db::repositories::tipcards::create_pending_batch_if_needed(
        &db,
        TEST_USER_ID,
        own_topic,
        "casual_tip",
        0,
        &cards,
    )
    .await;
    assert!(matches!(
        mismatched_result,
        Err(crate::error::AppError::Validation(_))
    ));

    let count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tipcards WHERE topic_id = ANY($1)")
            .bind(vec![foreign_topic, own_topic])
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn generated_card_creation_rolls_back_when_review_state_insert_fails() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);
    let topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type)
         VALUES ($1, 'atomic creation', 'repeatable_tip') RETURNING id",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&state.db)
    .await
    .unwrap();

    sqlx::query(
        "CREATE FUNCTION reject_review_state_insert() RETURNS trigger
         LANGUAGE plpgsql AS $$
         BEGIN
             RAISE EXCEPTION 'forced review state failure';
         END
         $$",
    )
    .execute(&state.db)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER reject_review_state_insert
         BEFORE INSERT ON review_states
         FOR EACH ROW EXECUTE FUNCTION reject_review_state_insert()",
    )
    .execute(&state.db)
    .await
    .unwrap();

    let result = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "must roll back",
        "full",
        "compact",
        false,
        "",
        "pending",
    )
    .await;
    assert!(result.is_err());

    let card_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM tipcards WHERE user_id = $1 AND topic_id = $2",
    )
    .bind(TEST_USER_ID)
    .bind(topic_id)
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(card_count, 0);
}

#[tokio::test]
async fn concurrent_generated_batches_persist_only_once() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);
    let topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type)
         VALUES ($1, 'atomic batch', 'repeatable_tip') RETURNING id",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&state.db)
    .await
    .unwrap();

    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(3));
    let mut tasks = Vec::new();
    for _ in 0..2 {
        let pool = state.db.clone();
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            let cards = [
                crate::db::repositories::tipcards::GeneratedCardParams {
                    title: "batch one",
                    full_content: "full one",
                    compressed_content: "compact one",
                    use_image: false,
                    image_query: "",
                },
                crate::db::repositories::tipcards::GeneratedCardParams {
                    title: "batch two",
                    full_content: "full two",
                    compressed_content: "compact two",
                    use_image: false,
                    image_query: "",
                },
            ];
            crate::db::repositories::tipcards::create_pending_batch_if_needed(
                &pool,
                TEST_USER_ID,
                topic_id,
                "repeatable_tip",
                0,
                &cards,
            )
            .await
            .unwrap()
            .len()
        }));
    }
    barrier.wait().await;
    let first = tasks.remove(0).await.unwrap();
    let second = tasks.remove(0).await.unwrap();
    assert_eq!(first + second, 2);

    let pending = crate::db::repositories::tipcards::count_pending(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
    )
    .await
    .unwrap();
    assert_eq!(pending, 2);
}

#[tokio::test]
async fn flow_cursor_uses_timestamp_keyset_without_repeating_cards() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);
    let topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type)
         VALUES ($1, 'cursor cards', 'casual_tip') RETURNING id",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&state.db)
    .await
    .unwrap();

    for index in 0..3 {
        crate::db::repositories::tipcards::create_generated_with_status(
            &state.db,
            TEST_USER_ID,
            topic_id,
            "casual_tip",
            &format!("cursor {index}"),
            "full",
            "compact",
            false,
            "",
            "active",
        )
        .await
        .unwrap();
    }

    let first =
        crate::db::repositories::tipcards::list_flow_cards(&state.db, TEST_USER_ID, None, 2)
            .await
            .unwrap();
    assert_eq!(first.len(), 2);
    let last = first.last().unwrap();
    let cursor = (i64::from(last.pinned), last.created_at.clone(), last.id);
    let second = crate::db::repositories::tipcards::list_flow_cards(
        &state.db,
        TEST_USER_ID,
        Some(cursor),
        2,
    )
    .await
    .unwrap();
    assert_eq!(second.len(), 1);
    assert!(first.iter().all(|card| card.id != second[0].id));
}

#[tokio::test]
async fn test_repeatable_topic_returns_one_new_card_after_review() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "daily_topic").await;

    let first_response = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Tips(
                crate::api::pb::TipsQuery {
                    count: 5,
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
                    grounding_strategy: None,
                    image_strategy: None,
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

    assert_eq!(second_tips.len(), 1);
    assert_ne!(second_tips[0].id, first.id);

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
async fn test_repeatable_tipcards_stop_after_the_default_daily_limit() {
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
    assert!(second_resp.tips.is_empty());
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
async fn test_manual_tipcards_store_update_and_delete_image_files() {
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

    let stored: String = sqlx::query_scalar("SELECT image_data FROM tipcards WHERE id = $1")
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
        sqlx::query_as("SELECT mime_type, byte_size FROM tipcard_images WHERE card_id = $1")
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
    let updated: String = sqlx::query_scalar("SELECT image_data FROM tipcards WHERE id = $1")
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
        sqlx::query_as("SELECT mime_type, byte_size FROM tipcard_images WHERE card_id = $1")
            .bind(tips[0].id)
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(updated_image.0, "image/webp");
    assert!(updated_image.1 > 0);

    let storage_path: String =
        sqlx::query_scalar("SELECT storage_path FROM tipcard_images WHERE card_id = $1")
            .bind(tips[0].id)
            .fetch_one(&state.db)
            .await
            .unwrap();
    let file_path = state.image_dir.join(&storage_path);
    assert!(fs::metadata(&file_path).await.is_ok());

    crate::services::tipcards::TipcardService::delete(&state, TEST_USER_ID, tips[0].id)
        .await
        .unwrap();
    assert!(matches!(
        fs::metadata(&file_path).await,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound
    ));
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
async fn test_concurrent_image_appends_respect_cap_positions_and_cleanup() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = std::sync::Arc::new(make_state(db, settings_path));
    let topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(TEST_USER_ID)
    .bind("images")
    .bind("manual_tip")
    .fetch_one(&state.db)
    .await
    .unwrap();
    let card_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(TEST_USER_ID)
    .bind(topic_id)
    .bind("manual_tip")
    .bind("Image cap")
    .bind("Image cap")
    .bind("Image cap")
    .fetch_one(&state.db)
    .await
    .unwrap();
    for position in 0..3 {
        sqlx::query(
            "INSERT INTO tipcard_images (user_id, card_id, position, storage_path, mime_type, byte_size)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(TEST_USER_ID)
        .bind(card_id)
        .bind(i64::from(position))
        .bind(format!("existing-{position}.png"))
        .bind("image/png")
        .bind(1_i64)
        .execute(&state.db)
        .await
        .unwrap();
    }

    let image = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScL0XwAAAABJRU5ErkJggg==";
    let first_state = state.clone();
    let second_state = state.clone();
    let (first, second) = tokio::join!(
        crate::services::tipcards::TipcardService::append_images(
            &first_state,
            TEST_USER_ID,
            card_id,
            vec![image.to_string()],
            vec![],
            vec![],
        ),
        crate::services::tipcards::TipcardService::append_images(
            &second_state,
            TEST_USER_ID,
            card_id,
            vec![image.to_string()],
            vec![],
            vec![],
        )
    );

    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    for result in [first, second] {
        if let Err((status, message)) = result {
            assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
            assert_eq!(message, "A tipcard can have at most 4 images");
        }
    }

    let images = crate::db::repositories::tipcards::list_images(&state.db, TEST_USER_ID, card_id)
        .await
        .unwrap();
    assert_eq!(images.len(), 4);
    assert_eq!(
        images
            .iter()
            .map(|image| image.position)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    let mut stored_files = fs::read_dir(&state.image_dir).await.unwrap();
    assert!(
        stored_files.next_entry().await.unwrap().is_some(),
        "the successful append stores its file"
    );
    assert!(
        stored_files.next_entry().await.unwrap().is_none(),
        "the rejected append cleans up its file"
    );
}

#[tokio::test]
async fn test_image_append_rejects_foreign_card_and_pool_image_before_writing() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);
    let other_user = "usr_other_images";
    sqlx::query("INSERT INTO users (id, username, password_hash, role) VALUES ($1, $2, $3, $4)")
        .bind(other_user)
        .bind("other-images")
        .bind("")
        .bind("user")
        .execute(&state.db)
        .await
        .unwrap();

    let foreign_topic = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(other_user)
    .bind("foreign")
    .bind("manual_tip")
    .fetch_one(&state.db)
    .await
    .unwrap();
    let foreign_card = sqlx::query_scalar::<_, i64>(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(other_user)
    .bind(foreign_topic)
    .bind("manual_tip")
    .bind("Foreign")
    .bind("Foreign")
    .bind("Foreign")
    .fetch_one(&state.db)
    .await
    .unwrap();
    let foreign_card_result = crate::services::tipcards::TipcardService::append_images(
        &state,
        TEST_USER_ID,
        foreign_card,
        vec!["data:image/png;base64,iVBORw0KGgo=".to_string()],
        vec![],
        vec![],
    )
    .await;
    assert_eq!(
        foreign_card_result.unwrap_err().0,
        axum::http::StatusCode::NOT_FOUND
    );

    let own_topic = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(TEST_USER_ID)
    .bind("own")
    .bind("manual_tip")
    .fetch_one(&state.db)
    .await
    .unwrap();
    let own_card = sqlx::query_scalar::<_, i64>(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(TEST_USER_ID)
    .bind(own_topic)
    .bind("manual_tip")
    .bind("Own")
    .bind("Own")
    .bind("Own")
    .fetch_one(&state.db)
    .await
    .unwrap();
    let foreign_pool_image = crate::db::repositories::image_pool::insert_pool_image(
        &state.db,
        other_user,
        "foreign.png",
        "image/png",
        1,
        "Foreign image",
        None,
        "[]",
    )
    .await
    .unwrap();
    let foreign_pool_result = crate::services::tipcards::TipcardService::append_images(
        &state,
        TEST_USER_ID,
        own_card,
        vec![],
        vec![foreign_pool_image],
        vec![],
    )
    .await;
    assert_eq!(
        foreign_pool_result.unwrap_err().0,
        axum::http::StatusCode::NOT_FOUND
    );
    assert!(
        !state.image_dir.exists(),
        "rejected requests do not write files"
    );
}

#[tokio::test]
async fn test_pinned_tipcard_is_returned_before_schedule() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);

    let topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ('usr_test_admin', $1, $2) RETURNING id",
    )
    .bind("spanish")
    .bind("repeatable_tip")
    .fetch_one(&state.db)
    .await
    .unwrap();
    let card_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO tipcards (user_id, topic_id, tipcard_type, title, full_content, compressed_content) VALUES ('usr_test_admin', $1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(topic_id)
    .bind("repeatable_tip")
    .bind("Pinned")
    .bind("Pinned full")
    .bind("Pinned compact")
    .fetch_one(&state.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO review_states (card_id, algorithm_used, state_data, status, next_review_at) VALUES ($1, $2, $3, 'active', $4)",
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
    .bind("Due")
    .bind("Due full")
    .bind("Due compact")
    .fetch_one(&state.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO review_states (card_id, algorithm_used, state_data, status, next_review_at) VALUES ($1, $2, $3, 'active', $4)",
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

    let pending_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "Pending",
        "Pending full",
        "Pending compact",
        false,
        "",
        "pending",
    )
    .await
    .unwrap();
    let blocked_promotion = crate::api::build_tips(
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
    assert!(blocked_promotion.is_empty());
    let pending_status: String =
        sqlx::query_scalar("SELECT status FROM review_states WHERE card_id = $1")
            .bind(pending_id)
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(pending_status, "pending");

    crate::api::apply_review(&state, TEST_USER_ID, card_id, 1, "again")
        .await
        .unwrap();
    let filler_topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ('usr_test_admin', 'capacity filler', 'manual_tip') RETURNING id",
    )
    .fetch_one(&state.db)
    .await
    .unwrap();
    crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        filler_topic_id,
        "manual_tip",
        "Capacity filler",
        "Capacity filler full",
        "Capacity filler compact",
        false,
        "",
        "active",
    )
    .await
    .unwrap();
    assert_eq!(
        crate::db::repositories::tipcards::active_card_count(&state.db, TEST_USER_ID)
            .await
            .unwrap(),
        1,
        "another due card should fill the active-card limit"
    );
    let expected_pending_id: i64 = sqlx::query_scalar(
        "SELECT r.card_id
         FROM review_states r
         JOIN tipcards t ON t.id = r.card_id
         WHERE t.user_id = $1 AND t.topic_id = $2 AND r.status = 'pending'
         ORDER BY t.created_at ASC, t.id ASC
         LIMIT 1",
    )
    .bind(TEST_USER_ID)
    .bind(topic_id)
    .fetch_one(&state.db)
    .await
    .unwrap();
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
    assert_ne!(promoted[0].id, card_id);
    assert_eq!(promoted[0].id, expected_pending_id);
    assert_ne!(promoted[0].id, pending_id);

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
async fn repeatable_review_refills_an_empty_queue_at_active_card_limit() {
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
        "INSERT INTO topics (user_id, name, tipcard_type, daily_card_count)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(TEST_USER_ID)
    .bind("English Grammar")
    .bind("repeatable_tip")
    .bind(2_i64)
    .fetch_one(&state.db)
    .await
    .unwrap();
    let reviewed_card_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "Present perfect",
        "Use the present perfect for unfinished time periods.",
        "Present perfect for unfinished time periods.",
        false,
        "",
        "active",
    )
    .await
    .unwrap();
    assert_eq!(
        crate::db::repositories::tipcards::active_card_count(&state.db, TEST_USER_ID)
            .await
            .unwrap(),
        1
    );

    crate::api::apply_review(&state, TEST_USER_ID, reviewed_card_id, 5, "learned")
        .await
        .unwrap();
    let filler_topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(TEST_USER_ID)
    .bind("Capacity filler")
    .bind("manual_tip")
    .fetch_one(&state.db)
    .await
    .unwrap();
    crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        filler_topic_id,
        "manual_tip",
        "Keep the cap full",
        "This due card occupies the configured active-card limit.",
        "This due card occupies the configured active-card limit.",
        false,
        "",
        "active",
    )
    .await
    .unwrap();
    assert_eq!(
        crate::db::repositories::tipcards::active_card_count(&state.db, TEST_USER_ID)
            .await
            .unwrap(),
        1,
        "another due card fills the active-card limit after the review"
    );

    let replacement = crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(1),
            topics: "English Grammar".into(),
            tipcard_type: Some("repeatable_tip".into()),
            exclude_card_ids: Some(vec![reviewed_card_id]),
            manual_content: None,
            manual_compressed_content: None,
            manual_image_data: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(replacement.len(), 1);
    assert_ne!(replacement[0].id, reviewed_card_id);
    assert_eq!(
        crate::db::repositories::tipcards::active_card_count(&state.db, TEST_USER_ID)
            .await
            .unwrap(),
        2,
        "the scheduled review and its immediate replacement may coexist"
    );
}

#[tokio::test]
async fn repeatable_cards_stop_at_the_topic_daily_limit_until_continued() {
    let settings_path = unique_settings_path();
    fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
        .await
        .unwrap();
    let db = setup_db().await;
    let state = make_state(db, settings_path);
    let topic_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO topics (user_id, name, tipcard_type, daily_card_count)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(TEST_USER_ID)
    .bind("English Grammar")
    .bind("repeatable_tip")
    .bind(2_i64)
    .fetch_one(&state.db)
    .await
    .unwrap();

    let first_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "First card",
        "First daily card",
        "First daily card",
        false,
        "",
        "active",
    )
    .await
    .unwrap();
    let second_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "Second card",
        "Second daily card",
        "Second daily card",
        false,
        "",
        "pending",
    )
    .await
    .unwrap();
    let third_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "Third card",
        "Third daily card",
        "Third daily card",
        false,
        "",
        "pending",
    )
    .await
    .unwrap();
    let fourth_id = crate::db::repositories::tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "Fourth card",
        "Fourth daily card",
        "Fourth daily card",
        false,
        "",
        "pending",
    )
    .await
    .unwrap();

    crate::api::apply_review(&state, TEST_USER_ID, first_id, 5, "learned")
        .await
        .unwrap();
    let second = crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(1),
            topics: "English Grammar".into(),
            tipcard_type: Some("repeatable_tip".into()),
            exclude_card_ids: Some(vec![first_id]),
            manual_content: None,
            manual_compressed_content: None,
            manual_image_data: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        second.iter().map(|card| card.id).collect::<Vec<_>>(),
        vec![second_id]
    );

    crate::api::apply_review(&state, TEST_USER_ID, second_id, 5, "learned")
        .await
        .unwrap();
    let stopped = crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(1),
            topics: "English Grammar".into(),
            tipcard_type: Some("repeatable_tip".into()),
            exclude_card_ids: Some(vec![second_id]),
            manual_content: None,
            manual_compressed_content: None,
            manual_image_data: None,
        },
    )
    .await
    .unwrap();
    assert!(
        stopped.is_empty(),
        "the final daily review must not advance itself"
    );

    let flow_cards =
        crate::services::tipcards::TipcardService::list_flow_cards(&state, TEST_USER_ID, None, 48)
            .await
            .unwrap();
    assert!(
        !flow_cards.iter().any(|card| card.id == third_id),
        "a page refresh must not bypass the daily limit"
    );

    crate::api::tips::continue_daily_review(
        &state,
        TEST_USER_ID,
        crate::api::ContinueDailyReviewRequest {
            topics: "English Grammar".into(),
            tipcard_type: Some("repeatable_tip".into()),
        },
    )
    .await
    .unwrap();
    let continued =
        crate::services::tipcards::TipcardService::list_flow_cards(&state, TEST_USER_ID, None, 48)
            .await
            .unwrap();
    assert!(
        continued.iter().any(|card| card.id == third_id),
        "Continue must show the first card in the next full set"
    );

    crate::api::apply_review(&state, TEST_USER_ID, third_id, 5, "learned")
        .await
        .unwrap();
    let fourth = crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(1),
            topics: "English Grammar".into(),
            tipcard_type: Some("repeatable_tip".into()),
            exclude_card_ids: Some(vec![third_id]),
            manual_content: None,
            manual_compressed_content: None,
            manual_image_data: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        fourth.iter().map(|card| card.id).collect::<Vec<_>>(),
        vec![fourth_id],
        "Continue must add the topic's full daily-card count, not just one card"
    );

    crate::api::apply_review(&state, TEST_USER_ID, fourth_id, 5, "learned")
        .await
        .unwrap();
    let final_stop = crate::api::build_tips(
        &state,
        TEST_USER_ID,
        crate::api::TipsJsonRequest {
            count: Some(1),
            topics: "English Grammar".into(),
            tipcard_type: Some("repeatable_tip".into()),
            exclude_card_ids: Some(vec![fourth_id]),
            manual_content: None,
            manual_compressed_content: None,
            manual_image_data: None,
        },
    )
    .await
    .unwrap();
    assert!(
        final_stop.is_empty(),
        "the continued set must stop after its configured number of cards"
    );
}

#[tokio::test]
async fn test_app_tip_replacement_excludes_visible_cards() {
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

    let mut visible_ids = Vec::new();
    for label in ["one", "two"] {
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
            "INSERT INTO review_states (card_id, algorithm_used, state_data, status, next_review_at) VALUES ($1, $2, $3, 'active', $4)",
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
