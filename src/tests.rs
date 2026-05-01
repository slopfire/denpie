#[cfg(test)]
mod tests {
    use crate::{apply_schema_migrations, build_app, AppState};
    use prost::Message;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::fs;
    use tower_sessions::{MemoryStore, SessionManagerLayer};

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let schema = tokio::fs::read_to_string("schema.sql")
            .await
            .unwrap_or_default();
        for query in schema.split(';') {
            if !query.trim().is_empty() {
                sqlx::query(query).execute(&pool).await.unwrap();
            }
        }
        apply_schema_migrations(&pool).await.unwrap();
        pool
    }

    /// Write isolated test settings and spin up a real server on an ephemeral port.
    /// Returns (base_url, reqwest::Client with cookie jar).
    async fn spawn_test_server() -> (String, reqwest::Client) {
        let test_token = "test_admin_token_xyz";
        let settings_path = unique_settings_path();
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            serde_yaml::Value::String("admin_token".into()),
            serde_yaml::Value::String(test_token.into()),
        );
        let settings_val = serde_yaml::Value::Mapping(map);
        fs::write(
            &settings_path,
            serde_yaml::to_string(&settings_val).unwrap(),
        )
        .await
        .unwrap();

        let db = setup_db().await;
        let state = Arc::new(AppState {
            db,
            settings_path,
            template_dir: PathBuf::from("templates"),
        });
        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store).with_secure(false);
        let app = build_app(state, session_layer);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{}", addr);

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        (base_url, client)
    }

    fn unique_settings_path() -> PathBuf {
        let suffix: u64 = rand::random();
        std::env::temp_dir().join(format!("dailytipdraft-test-settings-{suffix}.yaml"))
    }

    async fn bootstrap_api_key(url: &str, client: &reqwest::Client, client_name: &str) -> String {
        let request = crate::api::pb::ApiRequest {
            auth: "".into(),
            op: Some(crate::api::pb::api_request::Op::BootstrapApiKey(
                crate::api::pb::BootstrapApiKeyRequest {
                    admin_token: "test_admin_token_xyz".into(),
                    client_name: client_name.into(),
                },
            )),
        };
        let response = client
            .post(format!("{url}/api"))
            .header("Content-Type", "application/x-protobuf")
            .body(request.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let response =
            crate::api::pb::ApiResponse::decode(response.bytes().await.unwrap()).unwrap();
        match response.result.unwrap() {
            crate::api::pb::api_response::Result::ApiKeyCreated(created) => created.api_key,
            other => panic!("unexpected response: {:?}", other),
        }
    }

    async fn post_api(
        url: &str,
        client: &reqwest::Client,
        request: crate::api::pb::ApiRequest,
    ) -> reqwest::Response {
        client
            .post(format!("{url}/api"))
            .header("Content-Type", "application/x-protobuf")
            .body(request.encode_to_vec())
            .send()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_legacy_api_routes_are_removed() {
        let (url, client) = spawn_test_server().await;
        let routes = [
            ("POST", "/tips"),
            ("GET", "/topics"),
            ("GET", "/topic-classes"),
            ("POST", "/review"),
            ("GET", "/admin"),
        ];

        for (method, path) in routes {
            let request = match method {
                "GET" => client.get(format!("{url}{path}")),
                "POST" => client.post(format!("{url}{path}")),
                _ => unreachable!(),
            };
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND, "{path}");
        }
    }

    #[tokio::test]
    async fn test_root_page_serves_html() {
        let (url, client) = spawn_test_server().await;
        let response = client.get(format!("{url}/")).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let body = response.text().await.unwrap();
        assert!(body.contains("MindLift SRS"));
        assert!(body.contains("admin-token"));
        assert!(body.contains("/app/tips"));
    }

    #[tokio::test]
    async fn test_unified_protobuf_api_bootstrap_and_manage() {
        let (url, client) = spawn_test_server().await;

        let bootstrap = crate::api::pb::ApiRequest {
            auth: "".into(),
            op: Some(crate::api::pb::api_request::Op::BootstrapApiKey(
                crate::api::pb::BootstrapApiKeyRequest {
                    admin_token: "test_admin_token_xyz".into(),
                    client_name: "unified".into(),
                },
            )),
        };
        let res = client
            .post(format!("{url}/api"))
            .header("Content-Type", "application/x-protobuf")
            .body(bootstrap.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let resp = crate::api::pb::ApiResponse::decode(res.bytes().await.unwrap()).unwrap();
        let api_key = match resp.result.unwrap() {
            crate::api::pb::api_response::Result::ApiKeyCreated(created) => created.api_key,
            other => panic!("unexpected response: {:?}", other),
        };
        assert!(api_key.starts_with("sk_live_"));

        let update_settings = crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::UpdateSettings(
                crate::api::pb::UpdateSettingsRequest {
                    model: Some("google/gemini-2.5-pro".into()),
                    color_scheme: Some("solarized".into()),
                    daily_time_zone: Some("UTC+10".into()),
                    daily_update_time: Some("06:30".into()),
                    ..Default::default()
                },
            )),
        };
        let res = client
            .post(format!("{url}/api"))
            .body(update_settings.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);

        let get_settings = crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::GetSettings(
                crate::api::pb::Empty {},
            )),
        };
        let res = client
            .post(format!("{url}/api"))
            .body(get_settings.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let resp = crate::api::pb::ApiResponse::decode(res.bytes().await.unwrap()).unwrap();
        match resp.result.unwrap() {
            crate::api::pb::api_response::Result::Settings(settings) => {
                assert_eq!(settings.model, "google/gemini-2.5-pro");
                assert_eq!(settings.color_scheme, "solarized");
                assert_eq!(settings.daily_time_zone, "UTC+10");
                assert_eq!(settings.daily_update_time, "06:30");
            }
            other => panic!("unexpected response: {:?}", other),
        }

        let tips = crate::api::pb::ApiRequest {
            auth: api_key,
            op: Some(crate::api::pb::api_request::Op::Tips(
                crate::api::pb::TipsQuery {
                    count: 1,
                    topics: "rust".into(),
                    topic_class: "default".into(),
                    tipcard_type: "srs_tip".into(),
                    exclude_card_ids: vec![],
                },
            )),
        };
        let res = client
            .post(format!("{url}/api"))
            .body(tips.encode_to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let resp = crate::api::pb::ApiResponse::decode(res.bytes().await.unwrap()).unwrap();
        match resp.result.unwrap() {
            crate::api::pb::api_response::Result::Tips(tips) => {
                assert_eq!(tips.tips.len(), 1);
                assert_eq!(tips.tips[0].topic, "rust");
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_unified_tip_review_flow() {
        let (url, client) = spawn_test_server().await;
        let api_key = bootstrap_api_key(&url, &client, "browser_flow").await;

        let res = post_api(
            &url,
            &client,
            crate::api::pb::ApiRequest {
                auth: api_key.clone(),
                op: Some(crate::api::pb::api_request::Op::Tips(
                    crate::api::pb::TipsQuery {
                        count: 1,
                        topics: "rust".into(),
                        topic_class: "casual".into(),
                        tipcard_type: "casual_tip".into(),
                        exclude_card_ids: vec![],
                    },
                )),
            },
        )
        .await;
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let resp = crate::api::pb::ApiResponse::decode(res.bytes().await.unwrap()).unwrap();
        let first = match resp.result.unwrap() {
            crate::api::pb::api_response::Result::Tips(tips) => tips.tips[0].clone(),
            other => panic!("unexpected response: {:?}", other),
        };
        assert_eq!(first.topic, "rust");
        assert_eq!(first.topic_class, "casual");
        assert_eq!(first.tipcard_type, "casual_tip");

        let review = post_api(
            &url,
            &client,
            crate::api::pb::ApiRequest {
                auth: api_key.clone(),
                op: Some(crate::api::pb::api_request::Op::Review(
                    crate::api::pb::ReviewPayload {
                        card_id: first.id,
                        grade: 3,
                        action: "acknowledge".into(),
                    },
                )),
            },
        )
        .await;
        assert_eq!(review.status(), reqwest::StatusCode::OK);

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
        let resp = crate::api::pb::ApiResponse::decode(summary.bytes().await.unwrap()).unwrap();
        match resp.result.unwrap() {
            crate::api::pb::api_response::Result::Summary(summary) => {
                assert_eq!(summary.topics, 1);
                assert_eq!(summary.total_cards, 1);
            }
            other => panic!("unexpected response: {:?}", other),
        }
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
                        topic_class: "default".into(),
                        tipcard_type: "srs_tip".into(),
                        exclude_card_ids: vec![],
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
                        topic_class: "default".into(),
                        tipcard_type: "srs_tip".into(),
                        exclude_card_ids: vec![],
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
    }

    #[tokio::test]
    async fn test_unified_api_can_delete_tipcard() {
        let (url, client) = spawn_test_server().await;
        let api_key = bootstrap_api_key(&url, &client, "delete_flow").await;

        let tips = post_api(
            &url,
            &client,
            crate::api::pb::ApiRequest {
                auth: api_key.clone(),
                op: Some(crate::api::pb::api_request::Op::Tips(
                    crate::api::pb::TipsQuery {
                        count: 1,
                        topics: "rust".into(),
                        topic_class: "repeatable".into(),
                        tipcard_type: "repeatable_tip".into(),
                        exclude_card_ids: vec![],
                    },
                )),
            },
        )
        .await;
        let resp = crate::api::pb::ApiResponse::decode(tips.bytes().await.unwrap()).unwrap();
        let card_id = match resp.result.unwrap() {
            crate::api::pb::api_response::Result::Tips(tips) => tips.tips[0].id,
            other => panic!("unexpected response: {:?}", other),
        };

        let delete = post_api(
            &url,
            &client,
            crate::api::pb::ApiRequest {
                auth: api_key.clone(),
                op: Some(crate::api::pb::api_request::Op::DeleteTipcard(
                    crate::api::pb::DeleteByIdRequest { id: card_id },
                )),
            },
        )
        .await;
        assert_eq!(delete.status(), reqwest::StatusCode::OK);

        let cards = post_api(
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
        let resp = crate::api::pb::ApiResponse::decode(cards.bytes().await.unwrap()).unwrap();
        match resp.result.unwrap() {
            crate::api::pb::api_response::Result::Tipcards(cards) => {
                assert!(cards.cards.iter().all(|card| card.id != card_id));
            }
            other => panic!("unexpected response: {:?}", other),
        }

        let review = post_api(
            &url,
            &client,
            crate::api::pb::ApiRequest {
                auth: api_key,
                op: Some(crate::api::pb::api_request::Op::Review(
                    crate::api::pb::ReviewPayload {
                        card_id,
                        grade: 3,
                        action: "dismiss".into(),
                    },
                )),
            },
        )
        .await;
        assert_eq!(review.status(), reqwest::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_unified_api_can_delete_topic_with_cards() {
        let (url, client) = spawn_test_server().await;
        let api_key = bootstrap_api_key(&url, &client, "delete_topic_flow").await;

        let tips = post_api(
            &url,
            &client,
            crate::api::pb::ApiRequest {
                auth: api_key.clone(),
                op: Some(crate::api::pb::api_request::Op::Tips(
                    crate::api::pb::TipsQuery {
                        count: 1,
                        topics: "rust".into(),
                        topic_class: "repeatable".into(),
                        tipcard_type: "repeatable_tip".into(),
                        exclude_card_ids: vec![],
                    },
                )),
            },
        )
        .await;
        assert_eq!(tips.status(), reqwest::StatusCode::OK);

        let topics = post_api(
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
        let topic_id = crate::api::pb::ApiResponse::decode(topics.bytes().await.unwrap())
            .unwrap()
            .result
            .and_then(|result| match result {
                crate::api::pb::api_response::Result::AppTopics(topics) => {
                    topics.topics.into_iter().find(|topic| topic.name == "rust")
                }
                _ => None,
            })
            .expect("rust topic")
            .id;

        let delete = post_api(
            &url,
            &client,
            crate::api::pb::ApiRequest {
                auth: api_key.clone(),
                op: Some(crate::api::pb::api_request::Op::DeleteTopic(
                    crate::api::pb::DeleteByIdRequest { id: topic_id },
                )),
            },
        )
        .await;
        assert_eq!(delete.status(), reqwest::StatusCode::OK);

        let topics = post_api(
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
        let topics = crate::api::pb::ApiResponse::decode(topics.bytes().await.unwrap()).unwrap();
        match topics.result.unwrap() {
            crate::api::pb::api_response::Result::AppTopics(topics) => {
                assert!(topics.topics.iter().all(|topic| topic.id != topic_id));
            }
            other => panic!("unexpected response: {:?}", other),
        }

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
        let cards = crate::api::pb::ApiResponse::decode(cards.bytes().await.unwrap()).unwrap();
        match cards.result.unwrap() {
            crate::api::pb::api_response::Result::Tipcards(cards) => {
                assert!(cards.cards.iter().all(|card| card.topic_name != "rust"));
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_unified_api_with_invalid_key() {
        let (url, client) = spawn_test_server().await;
        let res = post_api(
            &url,
            &client,
            crate::api::pb::ApiRequest {
                auth: "sk_live_totallyFakeKeyBruh".into(),
                op: Some(crate::api::pb::api_request::Op::Tips(
                    crate::api::pb::TipsQuery {
                        count: 1,
                        topics: "rust".into(),
                        topic_class: "".into(),
                        tipcard_type: "".into(),
                        exclude_card_ids: vec![],
                    },
                )),
            },
        )
        .await;
        assert_eq!(res.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_unified_api_missing_auth() {
        let (url, client) = spawn_test_server().await;
        let res = post_api(
            &url,
            &client,
            crate::api::pb::ApiRequest {
                auth: "".into(),
                op: Some(crate::api::pb::api_request::Op::Tips(
                    crate::api::pb::TipsQuery {
                        count: 1,
                        topics: "rust".into(),
                        topic_class: "".into(),
                        tipcard_type: "".into(),
                        exclude_card_ids: vec![],
                    },
                )),
            },
        )
        .await;
        assert_eq!(res.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    // ──────────────────────────────────────────────
    //  Full API Flow: tips → review → review-not-found
    // ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_full_api_flow() {
        let (url, client) = spawn_test_server().await;
        let api_key = bootstrap_api_key(&url, &client, "flow_test").await;

        // Fetch tips
        let tips_query = crate::api::pb::TipsQuery {
            count: 1,
            topics: "rust".into(),
            topic_class: "".into(),
            tipcard_type: "".into(),
            exclude_card_ids: vec![],
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
        let card_id = tips_resp.tips[0].id;
        assert!(!tips_resp.tips[0].full_content.is_empty());
        assert!(!tips_resp.tips[0].compressed_content.is_empty());
        assert_eq!(tips_resp.tips[0].topic, "rust");

        // Review that card — should succeed
        let review = crate::api::pb::ReviewPayload {
            card_id,
            grade: 4,
            action: "".into(),
        };
        let res = post_api(
            &url,
            &client,
            crate::api::pb::ApiRequest {
                auth: api_key.clone(),
                op: Some(crate::api::pb::api_request::Op::Review(review)),
            },
        )
        .await;
        assert_eq!(res.status(), reqwest::StatusCode::OK);

        // Review a card that doesn't exist — should 404
        let ghost_review = crate::api::pb::ReviewPayload {
            card_id: 99999,
            grade: 3,
            action: "".into(),
        };
        let res = post_api(
            &url,
            &client,
            crate::api::pb::ApiRequest {
                auth: api_key,
                op: Some(crate::api::pb::api_request::Op::Review(ghost_review)),
            },
        )
        .await;
        assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);
    }

    // ──────────────────────────────────────────────
    //  Protobuf error handling
    // ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_tips_bad_protobuf_body() {
        let (url, client) = spawn_test_server().await;
        let res = client
            .post(format!("{url}/api"))
            .header("Content-Type", "application/x-protobuf")
            .body(vec![0xDE, 0xAD, 0xBE, 0xEF])
            .send()
            .await
            .unwrap();
        assert!(
            res.status() == reqwest::StatusCode::BAD_REQUEST
                || res.status() == reqwest::StatusCode::UNAUTHORIZED,
            "Should handle garbage protobuf gracefully, got {}",
            res.status()
        );
    }

    // ──────────────────────────────────────────────
    //  Multiple topics in one request
    // ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_tips_multiple_topics() {
        let (url, client) = spawn_test_server().await;
        let api_key = bootstrap_api_key(&url, &client, "multi_topic").await;

        let tips_query = crate::api::pb::TipsQuery {
            count: 3,
            topics: "rust, python, go".into(),
            topic_class: "".into(),
            tipcard_type: "".into(),
            exclude_card_ids: vec![],
        };
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
        let tips_resp = match api_resp.result.unwrap() {
            crate::api::pb::api_response::Result::Tips(tips) => tips,
            other => panic!("unexpected response: {:?}", other),
        };
        assert_eq!(tips_resp.tips.len(), 3);

        let topics: Vec<&str> = tips_resp.tips.iter().map(|t| t.topic.as_str()).collect();
        assert!(topics.contains(&"rust"));
        assert!(topics.contains(&"python"));
        assert!(topics.contains(&"go"));
    }

    #[tokio::test]
    async fn test_repeatable_tipcards_can_dismiss_and_get_new_card() {
        let (url, client) = spawn_test_server().await;
        let api_key = bootstrap_api_key(&url, &client, "repeatable_flow").await;

        let tips_query = crate::api::pb::TipsQuery {
            count: 1,
            topics: "spanish".into(),
            topic_class: "re:word".into(),
            tipcard_type: "repeatable_tip".into(),
            exclude_card_ids: vec![],
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
        assert_eq!(first_resp.tips[0].topic_class, "re:word");
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
    async fn test_app_tip_replacement_excludes_visible_cards() {
        let settings_path = unique_settings_path();
        fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
            .await
            .unwrap();
        let db = setup_db().await;
        let state = AppState {
            db,
            settings_path,
            template_dir: PathBuf::from("templates"),
        };

        let class_id = sqlx::query("INSERT INTO topic_classes (name, tipcard_type) VALUES (?, ?)")
            .bind("repeatable")
            .bind("repeatable_tip")
            .execute(&state.db)
            .await
            .unwrap()
            .last_insert_rowid();
        let topic_id = sqlx::query("INSERT INTO topics (name, class_id) VALUES (?, ?)")
            .bind("spanish")
            .bind(class_id)
            .execute(&state.db)
            .await
            .unwrap()
            .last_insert_rowid();

        let mut visible_ids = Vec::new();
        for label in ["one", "two"] {
            let card_id = sqlx::query(
                "INSERT INTO tipcards (topic_id, tipcard_type, title, full_content, compressed_content) VALUES (?, ?, ?, ?, ?)",
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

        crate::api::apply_review(&state, visible_ids[0], 3, "repeat")
            .await
            .unwrap();

        let replacement = crate::api::build_tips(
            &state,
            crate::api::TipsJsonRequest {
                count: Some(1),
                topics: "spanish".into(),
                topic_class: Some("repeatable".into()),
                tipcard_type: Some("repeatable_tip".into()),
                exclude_card_ids: Some(visible_ids.clone()),
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
    async fn test_repeatable_due_selection_prefers_known_cards() {
        let settings_path = unique_settings_path();
        fs::write(&settings_path, "admin_token: test_admin_token_xyz\n")
            .await
            .unwrap();
        let db = setup_db().await;
        let state = AppState {
            db,
            settings_path,
            template_dir: PathBuf::from("templates"),
        };

        let class_id = sqlx::query("INSERT INTO topic_classes (name, tipcard_type) VALUES (?, ?)")
            .bind("repeatable")
            .bind("repeatable_tip")
            .execute(&state.db)
            .await
            .unwrap()
            .last_insert_rowid();
        let topic_id = sqlx::query("INSERT INTO topics (name, class_id) VALUES (?, ?)")
            .bind("spanish")
            .bind(class_id)
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
                "INSERT INTO tipcards (topic_id, tipcard_type, title, full_content, compressed_content) VALUES (?, ?, ?, ?, ?)",
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
            .bind(format!(r#"{{"repeats":{repeats}}}"#))
            .bind(due_at)
            .execute(&state.db)
            .await
            .unwrap();
            card_ids.push(card_id);
        }

        let tips = crate::api::build_tips(
            &state,
            crate::api::TipsJsonRequest {
                count: Some(1),
                topics: "spanish".into(),
                topic_class: Some("repeatable".into()),
                tipcard_type: Some("repeatable_tip".into()),
                exclude_card_ids: None,
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
            topic_class: "casual".into(),
            tipcard_type: "casual_tip".into(),
            exclude_card_ids: vec![],
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
        assert_eq!(first_resp.tips[0].topic_class, "casual");
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
}
