use prost::Message;

use super::support::{bootstrap_api_key, post_api, post_api_v1, spawn_test_server};
use crate::api::pb;
use crate::llm::DEFAULT_PROMPT_TEMPLATE;

fn enhance_call(topic_id: i64) -> pb::ApiRequest {
    pb::ApiRequest {
        auth: String::new(),
        op: Some(pb::api_request::Op::EnhancePromptTemplate(
            pb::EnhancePromptTemplateRequest { topic_id },
        )),
    }
}

async fn decode_success(response: reqwest::Response) -> pb::EnhancePromptTemplateResult {
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let envelope = pb::ApiV1Response::decode(response.bytes().await.unwrap()).unwrap();
    match envelope.outcome.unwrap() {
        pb::api_v1_response::Outcome::Success(success) => match success.result.unwrap() {
            pb::api_response::Result::EnhancePromptTemplate(result) => result,
            other => panic!("unexpected result: {other:?}"),
        },
        other => panic!("unexpected outcome: {other:?}"),
    }
}

#[tokio::test]
async fn test_enhance_prompt_template_reads_card_history() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "prompt_enhance").await;

    let generated = post_api(
        &url,
        &client,
        pb::ApiRequest {
            auth: api_key.clone(),
            op: Some(pb::api_request::Op::Tips(pb::TipsQuery {
                count: 1,
                topics: "rust".into(),
                tipcard_type: "repeatable_tip".into(),
                exclude_card_ids: vec![],
                manual_content: "".into(),
                manual_compressed_content: "".into(),
            })),
        },
    )
    .await;
    assert_eq!(generated.status(), reqwest::StatusCode::OK);

    let topics = post_api_v1(
        &url,
        &client,
        Some(&api_key),
        "prompt-enhance-topics",
        pb::ApiRequest {
            auth: String::new(),
            op: Some(pb::api_request::Op::ListAppTopics(pb::Empty {})),
        },
    )
    .await;
    assert_eq!(topics.status(), reqwest::StatusCode::OK);
    let topics = pb::ApiV1Response::decode(topics.bytes().await.unwrap()).unwrap();
    let topic_id = match topics.outcome.unwrap() {
        pb::api_v1_response::Outcome::Success(success) => match success.result.unwrap() {
            pb::api_response::Result::AppTopics(topics) => topics.topics[0].id,
            other => panic!("unexpected topics result: {other:?}"),
        },
        other => panic!("unexpected topics outcome: {other:?}"),
    };

    let global = decode_success(
        post_api_v1(
            &url,
            &client,
            Some(&api_key),
            "prompt-enhance-global",
            enhance_call(0),
        )
        .await,
    )
    .await;
    assert!(global.prompt_template.contains("{topic}"));
    assert!(
        global.prompt_template.contains("tip cards")
            || global
                .prompt_template
                .contains(DEFAULT_PROMPT_TEMPLATE.trim())
            || !global.rationale.is_empty()
    );
    assert!(!global.rationale.is_empty());

    let topic = decode_success(
        post_api_v1(
            &url,
            &client,
            Some(&api_key),
            "prompt-enhance-topic",
            enhance_call(topic_id),
        )
        .await,
    )
    .await;
    assert!(topic.prompt_template.contains("{topic}"));
    assert!(!topic.rationale.is_empty());

    let missing = post_api_v1(
        &url,
        &client,
        Some(&api_key),
        "prompt-enhance-missing",
        enhance_call(i64::MAX),
    )
    .await;
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);

    let invalid = post_api_v1(
        &url,
        &client,
        Some(&api_key),
        "prompt-enhance-invalid",
        enhance_call(-1),
    )
    .await;
    assert_eq!(invalid.status(), reqwest::StatusCode::BAD_REQUEST);

    let unauthenticated = post_api_v1(
        &url,
        &reqwest::Client::new(),
        None,
        "prompt-enhance-unauthenticated",
        enhance_call(0),
    )
    .await;
    assert_eq!(unauthenticated.status(), reqwest::StatusCode::UNAUTHORIZED);

    let create_read_key = post_api_v1(
        &url,
        &client,
        Some(&api_key),
        "prompt-enhance-read-key",
        pb::ApiRequest {
            auth: String::new(),
            op: Some(pb::api_request::Op::CreateApiKeyV1(
                pb::CreateApiKeyV1Request {
                    client_name: "prompt enhance scope denial".to_string(),
                    scopes: vec!["cards:read".to_string()],
                    expires_at: String::new(),
                },
            )),
        },
    )
    .await;
    assert_eq!(create_read_key.status(), reqwest::StatusCode::OK);
    let create_read_key =
        pb::ApiV1Response::decode(create_read_key.bytes().await.unwrap()).unwrap();
    let read_key = match create_read_key.outcome.unwrap() {
        pb::api_v1_response::Outcome::Success(response) => match response.result.unwrap() {
            pb::api_response::Result::ApiKeyCreated(created) => created.api_key,
            other => panic!("unexpected key result: {other:?}"),
        },
        other => panic!("unexpected key outcome: {other:?}"),
    };
    let forbidden = post_api_v1(
        &url,
        &client,
        Some(&read_key),
        "prompt-enhance-forbidden",
        enhance_call(0),
    )
    .await;
    assert_eq!(forbidden.status(), reqwest::StatusCode::FORBIDDEN);
}
