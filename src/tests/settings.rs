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
            "reasoning_effort": "low",
            "compression_level": "strong",
            "color_scheme": "solarized-dark",
            "transparency": "full",
            "blur_intensity": "low",
            "daily_time_zone": "UTC+10",
            "daily_update_time": "06:30",
            "max_active_cards": 7
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
    assert_eq!(body["reasoning_effort"], "low");
    assert_eq!(body["compression_level"], "strong");
    assert_eq!(body["color_scheme"], "solarized-dark");
    assert_eq!(body["transparency"], "full");
    assert_eq!(body["blur_intensity"], "low");
    assert_eq!(body["daily_time_zone"], "UTC+10");
    assert_eq!(body["daily_update_time"], "06:30");
    assert_eq!(body["max_active_cards"], 7);
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
                max_active_cards: Some(7),
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
            assert_eq!(settings.max_active_cards, 7);
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
