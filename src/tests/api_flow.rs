use super::support::{
    bootstrap_api_key, post_api, post_api_v1, post_api_v1_with_idempotency, spawn_test_server,
};
use prost::Message;

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
                    tipcard_type: "casual_tip".into(),
                    exclude_card_ids: vec![],
                    manual_content: "".into(),
                    manual_compressed_content: "".into(),
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
            assert_eq!(summary.total_cards, 5);
        }
        other => panic!("unexpected response: {:?}", other),
    }
}

#[tokio::test]
async fn test_full_api_flow() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "flow_test").await;

    let tips_query = crate::api::pb::TipsQuery {
        count: 1,
        topics: "rust".into(),
        tipcard_type: "".into(),
        exclude_card_ids: vec![],
        manual_content: "".into(),
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
    let card_id = tips_resp.tips[0].id;
    assert!(!tips_resp.tips[0].full_content.is_empty());
    assert!(!tips_resp.tips[0].compressed_content.is_empty());
    assert_eq!(tips_resp.tips[0].topic, "rust");

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

#[tokio::test]
async fn test_tips_multiple_topics() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "multi_topic").await;

    let tips_query = crate::api::pb::TipsQuery {
        count: 3,
        topics: "rust, python, go".into(),
        tipcard_type: "".into(),
        exclude_card_ids: vec![],
        manual_content: "".into(),
        manual_compressed_content: "".into(),
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

                    tipcard_type: "repeatable_tip".into(),
                    exclude_card_ids: vec![],
                    manual_content: "".into(),
                    manual_compressed_content: "".into(),
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

                    tipcard_type: "repeatable_tip".into(),
                    exclude_card_ids: vec![],
                    manual_content: "".into(),
                    manual_compressed_content: "".into(),
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
async fn test_v1_discovery_is_unauthenticated_and_correlated() {
    let (url, client) = spawn_test_server().await;
    let response = post_api_v1(
        &url,
        &client,
        None,
        "discovery-1",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::GetApiInfo(
                crate::api::pb::Empty {},
            )),
        },
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/x-protobuf");
    assert_eq!(response.headers()["x-request-id"], "discovery-1");
    let response = crate::api::pb::ApiV1Response::decode(response.bytes().await.unwrap()).unwrap();
    assert_eq!(response.request_id, "discovery-1");
    let success = match response.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Success(success) => success,
        other => panic!("unexpected response: {other:?}"),
    };
    match success.result.unwrap() {
        crate::api::pb::api_response::Result::ApiInfo(info) => {
            assert_eq!(info.api_version, "v1");
            assert!(info.capabilities.iter().any(|item| item == "bearer_auth"));
            assert!(
                info.capabilities
                    .iter()
                    .any(|item| item == "structured_errors")
            );
            assert!(
                info.capabilities
                    .iter()
                    .any(|item| item == "durable_idempotency")
            );
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[tokio::test]
async fn test_v1_requires_protobuf_content_type() {
    let (url, client) = spawn_test_server().await;
    let response = client
        .post(format!("{url}/api/v1"))
        .body(Vec::<u8>::new())
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        reqwest::StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    let response = crate::api::pb::ApiV1Response::decode(response.bytes().await.unwrap()).unwrap();
    match response.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Error(error) => {
            assert_eq!(error.code, 7);
            assert!(!error.retryable);
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[tokio::test]
async fn test_v1_bearer_auth_and_structured_validation_errors() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "v1_bearer").await;

    let settings = post_api_v1(
        &url,
        &client,
        Some(&api_key),
        "settings-1",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::GetSettings(
                crate::api::pb::Empty {},
            )),
        },
    )
    .await;
    assert_eq!(settings.status(), reqwest::StatusCode::OK);

    let invalid = post_api_v1(
        &url,
        &client,
        Some(&api_key),
        "review-invalid",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::Review(
                crate::api::pb::ReviewPayload {
                    card_id: 1,
                    grade: 256,
                    action: "surprise_me".to_string(),
                },
            )),
        },
    )
    .await;
    assert_eq!(invalid.status(), reqwest::StatusCode::BAD_REQUEST);
    let invalid = crate::api::pb::ApiV1Response::decode(invalid.bytes().await.unwrap()).unwrap();
    assert_eq!(invalid.request_id, "review-invalid");
    match invalid.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Error(error) => {
            assert_eq!(error.code, 1);
            assert_eq!(error.message, "grade must be between 0 and 5");
            assert!(!error.retryable);
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[tokio::test]
async fn test_v1_flow_cards_are_paginated_and_have_detail() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "v1_flow").await;
    let tips = post_api(
        &url,
        &client,
        crate::api::pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(crate::api::pb::api_request::Op::Tips(
                crate::api::pb::TipsQuery {
                    count: 1,
                    topics: "rust".to_string(),
                    tipcard_type: "repeatable_tip".to_string(),
                    exclude_card_ids: vec![],
                    manual_content: String::new(),
                    manual_compressed_content: String::new(),
                },
            )),
        },
    )
    .await;
    let tips = crate::api::pb::ApiResponse::decode(tips.bytes().await.unwrap()).unwrap();
    let card_id = match tips.result.unwrap() {
        crate::api::pb::api_response::Result::Tips(tips) => tips.tips[0].id,
        other => panic!("unexpected response: {other:?}"),
    };

    let page = post_api_v1(
        &url,
        &client,
        Some(&api_key),
        "flow-page",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::ListFlowCards(
                crate::api::pb::ListFlowCardsRequest {
                    page_size: 1,
                    page_token: String::new(),
                },
            )),
        },
    )
    .await;
    let page = crate::api::pb::ApiV1Response::decode(page.bytes().await.unwrap()).unwrap();
    let page = match page.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Success(success) => match success.result.unwrap()
        {
            crate::api::pb::api_response::Result::FlowCardPage(page) => page,
            other => panic!("unexpected response: {other:?}"),
        },
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(page.cards.len(), 1);
    assert_eq!(page.cards[0].id, card_id);
    assert!(!page.cards[0].title.is_empty());

    let detail = post_api_v1(
        &url,
        &client,
        Some(&api_key),
        "card-detail",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::GetTipcard(
                crate::api::pb::GetByIdRequest { id: card_id },
            )),
        },
    )
    .await;
    assert_eq!(detail.status(), reqwest::StatusCode::OK);
    let detail = crate::api::pb::ApiV1Response::decode(detail.bytes().await.unwrap()).unwrap();
    match detail.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Success(success) => match success.result.unwrap()
        {
            crate::api::pb::api_response::Result::TipcardDetail(detail) => {
                assert_eq!(detail.card.unwrap().id, card_id);
            }
            other => panic!("unexpected response: {other:?}"),
        },
        other => panic!("unexpected response: {other:?}"),
    }
}

#[tokio::test]
async fn test_v1_scoped_keys_enforce_permissions_and_mask_secrets() {
    let (url, client) = spawn_test_server().await;
    let root_key = bootstrap_api_key(&url, &client, "scope_root").await;

    let set_secret = post_api_v1(
        &url,
        &client,
        Some(&root_key),
        "set-secret",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::UpdateSettings(
                crate::api::pb::UpdateSettingsRequest {
                    api_key: Some("top-secret".to_string()),
                    search_api_key: Some("search-secret".to_string()),
                    ..Default::default()
                },
            )),
        },
    )
    .await;
    assert_eq!(set_secret.status(), reqwest::StatusCode::OK);

    let create = post_api_v1(
        &url,
        &client,
        Some(&root_key),
        "create-scoped",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::CreateApiKeyV1(
                crate::api::pb::CreateApiKeyV1Request {
                    client_name: "read_only".to_string(),
                    scopes: vec!["cards:read".to_string(), "settings:read".to_string()],
                    expires_at: "2099-01-01T00:00:00Z".to_string(),
                },
            )),
        },
    )
    .await;
    assert_eq!(create.status(), reqwest::StatusCode::OK);
    let create = crate::api::pb::ApiV1Response::decode(create.bytes().await.unwrap()).unwrap();
    let scoped_key = match create.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Success(success) => {
            match success.result.unwrap() {
                crate::api::pb::api_response::Result::ApiKeyCreated(created) => created.api_key,
                other => panic!("unexpected response: {other:?}"),
            }
        }
        other => panic!("unexpected response: {other:?}"),
    };

    let settings = post_api_v1(
        &url,
        &client,
        Some(&scoped_key),
        "read-settings",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::GetSettings(
                crate::api::pb::Empty {},
            )),
        },
    )
    .await;
    let settings = crate::api::pb::ApiV1Response::decode(settings.bytes().await.unwrap()).unwrap();
    match settings.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Success(success) => {
            match success.result.unwrap() {
                crate::api::pb::api_response::Result::Settings(settings) => {
                    assert!(settings.api_key.is_empty());
                    assert!(settings.search_api_key.is_empty());
                }
                other => panic!("unexpected response: {other:?}"),
            }
        }
        other => panic!("unexpected response: {other:?}"),
    }

    let forbidden = post_api_v1(
        &url,
        &client,
        Some(&scoped_key),
        "write-settings",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::UpdateSettings(
                crate::api::pb::UpdateSettingsRequest {
                    model: Some("forbidden-model".to_string()),
                    ..Default::default()
                },
            )),
        },
    )
    .await;
    assert_eq!(forbidden.status(), reqwest::StatusCode::FORBIDDEN);
    let forbidden =
        crate::api::pb::ApiV1Response::decode(forbidden.bytes().await.unwrap()).unwrap();
    match forbidden.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Error(error) => {
            assert_eq!(error.code, 3);
            assert!(error.message.contains("settings:write"));
        }
        other => panic!("unexpected response: {other:?}"),
    }

    let create_manager = post_api_v1(
        &url,
        &client,
        Some(&root_key),
        "create-key-manager",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::CreateApiKeyV1(
                crate::api::pb::CreateApiKeyV1Request {
                    client_name: "delegated_manager".to_string(),
                    scopes: vec!["keys:manage".to_string()],
                    expires_at: "2099-01-01T00:00:00Z".to_string(),
                },
            )),
        },
    )
    .await;
    assert_eq!(create_manager.status(), reqwest::StatusCode::OK);
    let create_manager =
        crate::api::pb::ApiV1Response::decode(create_manager.bytes().await.unwrap()).unwrap();
    let manager_key = match create_manager.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Success(success) => {
            match success.result.unwrap() {
                crate::api::pb::api_response::Result::ApiKeyCreated(created) => created.api_key,
                other => panic!("unexpected response: {other:?}"),
            }
        }
        other => panic!("unexpected response: {other:?}"),
    };

    let escalated = post_api_v1(
        &url,
        &client,
        Some(&manager_key),
        "scope-escalation",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::CreateApiKeyV1(
                crate::api::pb::CreateApiKeyV1Request {
                    client_name: "too_broad".to_string(),
                    scopes: vec!["cards:read".to_string()],
                    expires_at: "2098-01-01T00:00:00Z".to_string(),
                },
            )),
        },
    )
    .await;
    assert_eq!(escalated.status(), reqwest::StatusCode::FORBIDDEN);

    let outlives_parent = post_api_v1(
        &url,
        &client,
        Some(&manager_key),
        "expiry-escalation",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::CreateApiKeyV1(
                crate::api::pb::CreateApiKeyV1Request {
                    client_name: "too_long".to_string(),
                    scopes: vec!["keys:manage".to_string()],
                    expires_at: String::new(),
                },
            )),
        },
    )
    .await;
    assert_eq!(outlives_parent.status(), reqwest::StatusCode::FORBIDDEN);

    let legacy_escalation = post_api_v1(
        &url,
        &client,
        Some(&manager_key),
        "legacy-key-escalation",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::CreateApiKey(
                crate::api::pb::CreateApiKeyRequest {
                    client_name: "legacy_full_access".to_string(),
                },
            )),
        },
    )
    .await;
    assert_eq!(legacy_escalation.status(), reqwest::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_v1_typed_cards_and_created_resources() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "v1_resources").await;

    let manual = post_api_v1(
        &url,
        &client,
        Some(&api_key),
        "manual-card",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::TipsV1(
                crate::api::pb::TipsRequestV1 {
                    count: 1,
                    topics: vec!["API design".to_string()],
                    tipcard_type: 3,
                    exclude_card_ids: vec![],
                    manual_content: "Prefer explicit contracts.".to_string(),
                    manual_compressed_content: "Explicit contracts.".to_string(),
                    manual_image_data: vec![],
                },
            )),
        },
    )
    .await;
    assert_eq!(manual.status(), reqwest::StatusCode::OK);
    let manual = crate::api::pb::ApiV1Response::decode(manual.bytes().await.unwrap()).unwrap();
    let card_id = match manual.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Success(success) => {
            match success.result.unwrap() {
                crate::api::pb::api_response::Result::Tips(tips) => tips.tips[0].id,
                other => panic!("unexpected response: {other:?}"),
            }
        }
        other => panic!("unexpected response: {other:?}"),
    };
    let review = post_api_v1(
        &url,
        &client,
        Some(&api_key),
        "typed-review",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::ReviewV1(
                crate::api::pb::ReviewRequestV1 {
                    card_id,
                    grade: 5,
                    action: 2,
                },
            )),
        },
    )
    .await;
    assert_eq!(review.status(), reqwest::StatusCode::OK);

    let document = post_api_v1(
        &url,
        &client,
        Some(&api_key),
        "create-document",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::CreateDocument(
                crate::api::pb::AddDocumentRequest {
                    topic_id_opt: String::new(),
                    source_type: "document".to_string(),
                    title: "API notes".to_string(),
                    url: String::new(),
                    content: "A stable API needs explicit versioning.".to_string(),
                    topic_ids: vec![],
                },
            )),
        },
    )
    .await;
    assert_eq!(document.status(), reqwest::StatusCode::OK);
    let document = crate::api::pb::ApiV1Response::decode(document.bytes().await.unwrap()).unwrap();
    match document.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Success(success) => {
            match success.result.unwrap() {
                crate::api::pb::api_response::Result::DocumentCreated(document) => {
                    assert!(document.id > 0);
                    assert_eq!(document.title, "API notes");
                    assert!(document.content.contains("versioning"));
                }
                other => panic!("unexpected response: {other:?}"),
            }
        }
        other => panic!("unexpected response: {other:?}"),
    }

    let upload = post_api_v1(
        &url,
        &client,
        Some(&api_key),
        "upload-document",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::UploadDocument(
                crate::api::pb::UploadDocumentRequest {
                    topic_ids: vec![],
                    filename: "notes.txt".to_string(),
                    mime_type: "text/plain".to_string(),
                    title: String::new(),
                    data: b"Uploaded API documentation".to_vec(),
                },
            )),
        },
    )
    .await;
    assert_eq!(upload.status(), reqwest::StatusCode::OK);

    let pool_image = post_api_v1(
        &url,
        &client,
        Some(&api_key),
        "create-pool-image",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::CreatePoolImage(
                crate::api::pb::AddPoolImageRequest {
                    image_data: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==".to_string(),
                    name: "red pixel".to_string(),
                    description: String::new(),
                },
            )),
        },
    )
    .await;
    assert_eq!(pool_image.status(), reqwest::StatusCode::OK);
    let pool_image =
        crate::api::pb::ApiV1Response::decode(pool_image.bytes().await.unwrap()).unwrap();
    let download_path = match pool_image.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Success(success) => {
            match success.result.unwrap() {
                crate::api::pb::api_response::Result::PoolImageCreated(image) => {
                    assert!(image.id > 0);
                    assert!(!image.download_path.is_empty());
                    image.download_path
                }
                other => panic!("unexpected response: {other:?}"),
            }
        }
        other => panic!("unexpected response: {other:?}"),
    };
    let image = client
        .get(format!("{url}{download_path}"))
        .bearer_auth(&api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(image.status(), reqwest::StatusCode::OK);
    assert_eq!(image.headers()["content-type"], "image/png");
    assert!(!image.bytes().await.unwrap().is_empty());
}

#[tokio::test]
async fn test_v1_idempotency_replays_results_and_rejects_payload_conflicts() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "idempotency_root").await;
    let create_document = crate::api::pb::ApiRequest {
        auth: String::new(),
        op: Some(crate::api::pb::api_request::Op::CreateDocument(
            crate::api::pb::AddDocumentRequest {
                topic_id_opt: String::new(),
                source_type: "document".to_string(),
                title: "Idempotent document".to_string(),
                url: String::new(),
                content: "Create this exactly once.".to_string(),
                topic_ids: vec![],
            },
        )),
    };

    let first = post_api_v1_with_idempotency(
        &url,
        &client,
        Some(&api_key),
        "idem-first",
        "document-operation-1",
        create_document.clone(),
    )
    .await;
    assert_eq!(first.status(), reqwest::StatusCode::OK);
    assert_eq!(first.headers()["idempotency-key"], "document-operation-1");
    assert!(first.headers().get("idempotency-replayed").is_none());
    let first = crate::api::pb::ApiV1Response::decode(first.bytes().await.unwrap()).unwrap();
    let first_id = match first.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Success(success) => {
            match success.result.unwrap() {
                crate::api::pb::api_response::Result::DocumentCreated(document) => document.id,
                other => panic!("unexpected response: {other:?}"),
            }
        }
        other => panic!("unexpected response: {other:?}"),
    };

    let replay = post_api_v1_with_idempotency(
        &url,
        &client,
        Some(&api_key),
        "idem-replay",
        "document-operation-1",
        create_document,
    )
    .await;
    assert_eq!(replay.status(), reqwest::StatusCode::OK);
    assert_eq!(replay.headers()["idempotency-replayed"], "true");
    let replay = crate::api::pb::ApiV1Response::decode(replay.bytes().await.unwrap()).unwrap();
    assert_eq!(replay.request_id, "idem-replay");
    let replay_id = match replay.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Success(success) => {
            match success.result.unwrap() {
                crate::api::pb::api_response::Result::DocumentCreated(document) => document.id,
                other => panic!("unexpected response: {other:?}"),
            }
        }
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(replay_id, first_id);

    let conflict = post_api_v1_with_idempotency(
        &url,
        &client,
        Some(&api_key),
        "idem-conflict",
        "document-operation-1",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::CreateDocument(
                crate::api::pb::AddDocumentRequest {
                    topic_id_opt: String::new(),
                    source_type: "document".to_string(),
                    title: "Different document".to_string(),
                    url: String::new(),
                    content: "This payload must not run.".to_string(),
                    topic_ids: vec![],
                },
            )),
        },
    )
    .await;
    assert_eq!(conflict.status(), reqwest::StatusCode::CONFLICT);
    let conflict = crate::api::pb::ApiV1Response::decode(conflict.bytes().await.unwrap()).unwrap();
    match conflict.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Error(error) => {
            assert_eq!(error.code, crate::api::pb::ApiErrorCode::Conflict as i32);
            assert!(!error.retryable);
            assert!(error.message.contains("different request"));
        }
        other => panic!("unexpected response: {other:?}"),
    }

    let listed = post_api_v1(
        &url,
        &client,
        Some(&api_key),
        "list-after-replay",
        crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::ListDocuments(
                crate::api::pb::Empty {},
            )),
        },
    )
    .await;
    let listed = crate::api::pb::ApiV1Response::decode(listed.bytes().await.unwrap()).unwrap();
    let documents = match listed.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Success(success) => {
            match success.result.unwrap() {
                crate::api::pb::api_response::Result::Documents(documents) => documents.docs,
                other => panic!("unexpected response: {other:?}"),
            }
        }
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(
        documents
            .iter()
            .filter(|document| document.title == "Idempotent document")
            .count(),
        1
    );
}

#[tokio::test]
async fn test_v1_idempotency_handles_concurrency_failures_and_required_keys() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "idempotency_concurrency").await;
    let concurrent_call = crate::api::pb::ApiRequest {
        auth: String::new(),
        op: Some(crate::api::pb::api_request::Op::CreateDocument(
            crate::api::pb::AddDocumentRequest {
                topic_id_opt: String::new(),
                source_type: "document".to_string(),
                title: "Concurrent idempotency".to_string(),
                url: String::new(),
                content: "Two transports, one mutation.".to_string(),
                topic_ids: vec![],
            },
        )),
    };
    let (left, right) = tokio::join!(
        post_api_v1_with_idempotency(
            &url,
            &client,
            Some(&api_key),
            "concurrent-left",
            "concurrent-document-1",
            concurrent_call.clone(),
        ),
        post_api_v1_with_idempotency(
            &url,
            &client,
            Some(&api_key),
            "concurrent-right",
            "concurrent-document-1",
            concurrent_call,
        )
    );
    assert_eq!(left.status(), reqwest::StatusCode::OK);
    assert_eq!(right.status(), reqwest::StatusCode::OK);
    let replay_count = [&left, &right]
        .into_iter()
        .filter(|response| {
            response
                .headers()
                .get("idempotency-replayed")
                .is_some_and(|value| value == "true")
        })
        .count();
    assert_eq!(replay_count, 1);
    let left = crate::api::pb::ApiV1Response::decode(left.bytes().await.unwrap()).unwrap();
    let right = crate::api::pb::ApiV1Response::decode(right.bytes().await.unwrap()).unwrap();
    let document_id = |response: crate::api::pb::ApiV1Response| match response.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Success(success) => {
            match success.result.unwrap() {
                crate::api::pb::api_response::Result::DocumentCreated(document) => document.id,
                other => panic!("unexpected response: {other:?}"),
            }
        }
        other => panic!("unexpected response: {other:?}"),
    };
    assert_eq!(document_id(left), document_id(right));

    let invalid_review = crate::api::pb::ApiRequest {
        auth: String::new(),
        op: Some(crate::api::pb::api_request::Op::ReviewV1(
            crate::api::pb::ReviewRequestV1 {
                card_id: 1,
                grade: 99,
                action: crate::api::pb::ReviewActionValue::Again as i32,
            },
        )),
    };
    let invalid = post_api_v1_with_idempotency(
        &url,
        &client,
        Some(&api_key),
        "invalid-first",
        "invalid-review-1",
        invalid_review.clone(),
    )
    .await;
    assert_eq!(invalid.status(), reqwest::StatusCode::BAD_REQUEST);
    let invalid_replay = post_api_v1_with_idempotency(
        &url,
        &client,
        Some(&api_key),
        "invalid-replay",
        "invalid-review-1",
        invalid_review,
    )
    .await;
    assert_eq!(invalid_replay.status(), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(invalid_replay.headers()["idempotency-replayed"], "true");

    let missing_key = client
        .post(format!("{url}/api/v1"))
        .header("Content-Type", "application/x-protobuf")
        .bearer_auth(&api_key)
        .body(
            crate::api::pb::ApiV1Request {
                request_id: "missing-idempotency".to_string(),
                call: Some(crate::api::pb::ApiRequest {
                    auth: String::new(),
                    op: Some(crate::api::pb::api_request::Op::ReviewV1(
                        crate::api::pb::ReviewRequestV1 {
                            card_id: 1,
                            grade: 5,
                            action: crate::api::pb::ReviewActionValue::Learned as i32,
                        },
                    )),
                }),
                idempotency_key: String::new(),
            }
            .encode_to_vec(),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(missing_key.status(), reqwest::StatusCode::BAD_REQUEST);

    let header_only_body = crate::api::pb::ApiV1Request {
        request_id: "header-only-first".to_string(),
        call: Some(crate::api::pb::ApiRequest {
            auth: String::new(),
            op: Some(crate::api::pb::api_request::Op::ReviewV1(
                crate::api::pb::ReviewRequestV1 {
                    card_id: 1,
                    grade: 98,
                    action: crate::api::pb::ReviewActionValue::Again as i32,
                },
            )),
        }),
        idempotency_key: String::new(),
    }
    .encode_to_vec();
    let header_only = client
        .post(format!("{url}/api/v1"))
        .header("Content-Type", "application/x-protobuf")
        .header("Idempotency-Key", "header-only-review-1")
        .bearer_auth(&api_key)
        .body(header_only_body.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(header_only.status(), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(
        header_only.headers()["idempotency-key"],
        "header-only-review-1"
    );
    let header_replay = client
        .post(format!("{url}/api/v1"))
        .header("Content-Type", "application/x-protobuf")
        .header("Idempotency-Key", "header-only-review-1")
        .bearer_auth(&api_key)
        .body(header_only_body)
        .send()
        .await
        .unwrap();
    assert_eq!(header_replay.status(), reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(header_replay.headers()["idempotency-replayed"], "true");

    let mismatch = client
        .post(format!("{url}/api/v1"))
        .header("Content-Type", "application/x-protobuf")
        .header("Idempotency-Key", "header-value")
        .bearer_auth(&api_key)
        .body(
            crate::api::pb::ApiV1Request {
                request_id: "mismatched-keys".to_string(),
                call: Some(crate::api::pb::ApiRequest {
                    auth: String::new(),
                    op: Some(crate::api::pb::api_request::Op::GetSummary(
                        crate::api::pb::Empty {},
                    )),
                }),
                idempotency_key: "body-value".to_string(),
            }
            .encode_to_vec(),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(mismatch.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_v1_one_time_api_key_results_are_not_stored_in_plaintext() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "one_time_root").await;
    let create_key = crate::api::pb::ApiRequest {
        auth: String::new(),
        op: Some(crate::api::pb::api_request::Op::CreateApiKeyV1(
            crate::api::pb::CreateApiKeyV1Request {
                client_name: "one_time_child".to_string(),
                scopes: vec!["cards:read".to_string()],
                expires_at: String::new(),
            },
        )),
    };
    let first = post_api_v1_with_idempotency(
        &url,
        &client,
        Some(&api_key),
        "one-time-first",
        "one-time-key-1",
        create_key.clone(),
    )
    .await;
    assert_eq!(first.status(), reqwest::StatusCode::OK);
    let first = crate::api::pb::ApiV1Response::decode(first.bytes().await.unwrap()).unwrap();
    match first.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Success(success) => {
            match success.result.unwrap() {
                crate::api::pb::api_response::Result::ApiKeyCreated(created) => {
                    assert!(created.api_key.starts_with("sk_live_"));
                }
                other => panic!("unexpected response: {other:?}"),
            }
        }
        other => panic!("unexpected response: {other:?}"),
    }

    let replay = post_api_v1_with_idempotency(
        &url,
        &client,
        Some(&api_key),
        "one-time-replay",
        "one-time-key-1",
        create_key,
    )
    .await;
    assert_eq!(replay.status(), reqwest::StatusCode::CONFLICT);
    let replay = crate::api::pb::ApiV1Response::decode(replay.bytes().await.unwrap()).unwrap();
    match replay.outcome.unwrap() {
        crate::api::pb::api_v1_response::Outcome::Error(error) => {
            assert!(!error.retryable);
            assert!(error.message.contains("one-time credential"));
        }
        other => panic!("unexpected response: {other:?}"),
    }
}
