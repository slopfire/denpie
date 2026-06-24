use super::support::{bootstrap_api_key, post_api, spawn_test_server};
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
            assert_eq!(summary.total_cards, 1);
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
