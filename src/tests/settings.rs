use super::support::spawn_test_server;
use prost::Message;

#[tokio::test]
async fn test_admin_settings_is_not_cacheable() {
    let (url, client) = spawn_test_server().await;
    let response = client
        .get(format!("{url}/admin/settings"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let cache_control = response
        .headers()
        .get(reqwest::header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert_eq!(cache_control, "no-store");
}

#[tokio::test]
async fn test_admin_settings_roundtrip_persists() {
    let (url, client) = spawn_test_server().await;

    let update = client
        .post(format!("{url}/admin/settings"))
        .json(&serde_json::json!({
            "model": "google/gemini-2.5-pro",
            "grounding_model": "openai/gpt-5-mini",
            "reasoning_effort": "low",
            "grounding_reasoning_effort": "high",
            "compression_level": "strong",
            "daily_time_zone": "UTC+10",
            "daily_update_time": "06:30",
            "max_active_cards": 7,
            "search_provider": "firecrawl",
            "scrape_provider": "firecrawl",
            "search_base_url": "https://api.firecrawl.dev",
            "search_api_key": "fc-test"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(update.status(), reqwest::StatusCode::OK);

    let read = client
        .get(format!("{url}/admin/settings"))
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), reqwest::StatusCode::OK);
    let body = read.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["model"], "google/gemini-2.5-pro");
    assert_eq!(body["grounding_model"], "openai/gpt-5-mini");
    assert_eq!(body["reasoning_effort"], "low");
    assert_eq!(body["grounding_reasoning_effort"], "high");
    assert_eq!(body["compression_level"], "strong");
    assert_eq!(body["daily_time_zone"], "UTC+10");
    assert_eq!(body["daily_update_time"], "06:30");
    assert_eq!(body["max_active_cards"], 7);
    assert_eq!(body["search_provider"], "firecrawl");
    assert_eq!(body["scrape_provider"], "firecrawl");
    assert_eq!(body["search_base_url"], "https://api.firecrawl.dev");
    assert_eq!(body["search_api_key"], "fc-test");
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
                grounding_model: Some("openai/gpt-5-mini".into()),
                grounding_reasoning_effort: Some("high".into()),
                daily_time_zone: Some("UTC+10".into()),
                daily_update_time: Some("06:30".into()),
                max_active_cards: Some(7),
                search_provider: Some("firecrawl".into()),
                scrape_provider: Some("firecrawl".into()),
                search_base_url: Some("https://api.firecrawl.dev".into()),
                search_api_key: Some("fc-test".into()),
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
            assert_eq!(settings.grounding_model, "openai/gpt-5-mini");
            assert_eq!(settings.grounding_reasoning_effort, "high");
            assert_eq!(settings.daily_time_zone, "UTC+10");
            assert_eq!(settings.daily_update_time, "06:30");
            assert_eq!(settings.max_active_cards, 7);
            assert_eq!(settings.search_provider, "firecrawl");
            assert_eq!(settings.scrape_provider, "firecrawl");
            assert_eq!(settings.search_base_url, "https://api.firecrawl.dev");
            assert_eq!(settings.search_api_key, "fc-test");
        }
        other => panic!("unexpected response: {:?}", other),
    }

    let tips = crate::api::pb::ApiRequest {
        auth: api_key,
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
