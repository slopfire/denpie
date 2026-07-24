use super::support::{post_api, spawn_test_server};

#[tokio::test]
async fn test_dashboard_summary_route_uses_session_auth() {
    let (url, client) = spawn_test_server().await;
    let response = client
        .get(format!("{url}/app/summary"))
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "summary status {}",
        response.status()
    );

    let summary: serde_json::Value = response.json().await.unwrap();
    assert_eq!(summary["active_cards"], 0);
    assert_eq!(summary["due_cards"], 0);
}

#[tokio::test]
async fn test_update_me_profile() {
    let (url, client) = spawn_test_server().await;

    let avatar = "data:image/png;base64,iVBORw0KGgo=".to_string();
    let update = client
        .patch(format!("{url}/auth/me"))
        .json(&serde_json::json!({
            "display_name": "New Name",
            "avatar_data": avatar
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(update.status(), reqwest::StatusCode::OK);

    let me = client.get(format!("{url}/auth/me")).send().await.unwrap();
    assert_eq!(me.status(), reqwest::StatusCode::OK);
    let user: serde_json::Value = me.json().await.unwrap();
    assert_eq!(user["display_name"], "New Name");
    assert_eq!(user["avatar_data"], avatar);
}

#[tokio::test]
async fn test_update_me_invalid_avatar() {
    let (url, client) = spawn_test_server().await;

    let update = client
        .patch(format!("{url}/auth/me"))
        .json(&serde_json::json!({
            "avatar_data": "not a data uri"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(update.status(), reqwest::StatusCode::BAD_REQUEST);
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
                    tipcard_type: "".into(),
                    exclude_card_ids: vec![],
                    manual_content: "".into(),
                    manual_compressed_content: "".into(),
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
                    tipcard_type: "".into(),
                    exclude_card_ids: vec![],
                    manual_content: "".into(),
                    manual_compressed_content: "".into(),
                },
            )),
        },
    )
    .await;
    assert_eq!(res.status(), reqwest::StatusCode::UNAUTHORIZED);
}
