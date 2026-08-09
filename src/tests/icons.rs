use super::support::{bootstrap_api_key, post_api, spawn_test_server};

async fn create_topic(url: &str, client: &reqwest::Client, name: &str, api_key: &str) -> i64 {
    let res = post_api(
        url,
        client,
        crate::api::pb::ApiRequest {
            auth: api_key.to_string(),
            op: Some(crate::api::pb::api_request::Op::Tips(
                crate::api::pb::TipsQuery {
                    count: 1,
                    topics: name.to_string(),
                    tipcard_type: "".into(),
                    exclude_card_ids: vec![],
                    manual_content: "".into(),
                    manual_compressed_content: "".into(),
                },
            )),
        },
    )
    .await;
    assert_eq!(res.status(), reqwest::StatusCode::OK);

    let topics: Vec<serde_json::Value> = client
        .get(format!("{url}/app/topics"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    topics
        .iter()
        .find(|topic| topic["name"] == name)
        .expect("topic exists after tip generation")["id"]
        .as_i64()
        .unwrap()
}

/// The icon suggestions endpoint must return exactly five distinct
/// allowlisted icons, and picking one must persist it on the topic.
#[tokio::test]
async fn test_topic_icon_suggest_and_set() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "icon_test").await;
    let topic_id = create_topic(&url, &client, "rust", &api_key).await;

    let topics: Vec<serde_json::Value> = client
        .get(format!("{url}/app/topics"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let original_icon = topics
        .iter()
        .find(|topic| topic["id"].as_i64() == Some(topic_id))
        .expect("rust topic listed")["icon_id"]
        .as_str()
        .unwrap()
        .to_string();

    let suggestions = client
        .post(format!("{url}/app/topics/suggest-icons"))
        .json(&serde_json::json!({ "id": topic_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(suggestions.status(), reqwest::StatusCode::OK);
    let icons = suggestions.json::<serde_json::Value>().await.unwrap()["icons"]
        .as_array()
        .expect("icons array")
        .iter()
        .map(|icon| icon.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(icons.len(), 5, "exactly five suggestions");
    let mut distinct = icons.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), 5, "suggestions are distinct");

    let rerolled = client
        .post(format!("{url}/app/topics/suggest-icons"))
        .json(&serde_json::json!({
            "id": topic_id,
            "excluded_icons": icons,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(rerolled.status(), reqwest::StatusCode::OK);
    let rerolled_icons = rerolled.json::<serde_json::Value>().await.unwrap()["icons"]
        .as_array()
        .expect("rerolled icons array")
        .iter()
        .map(|icon| icon.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(rerolled_icons.len(), 5, "exactly five rerolled suggestions");
    assert!(
        rerolled_icons.iter().all(|icon| !icons.contains(icon)),
        "rerolled suggestions exclude the currently displayed icons"
    );

    let picked = icons[2].clone();
    let set = client
        .post(format!("{url}/app/topics/set-icon"))
        .json(&serde_json::json!({ "id": topic_id, "icon_id": picked }))
        .send()
        .await
        .unwrap();
    assert_eq!(set.status(), reqwest::StatusCode::OK);

    let topics: Vec<serde_json::Value> = client
        .get(format!("{url}/app/topics"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let updated = topics
        .iter()
        .find(|topic| topic["id"].as_i64() == Some(topic_id))
        .expect("topic still listed");
    assert_eq!(updated["icon_id"].as_str().unwrap(), picked);
    assert_ne!(picked, original_icon);
}

/// Setting an icon outside the allowlist must be rejected.
#[tokio::test]
async fn test_topic_icon_set_rejects_unknown_icon() {
    let (url, client) = spawn_test_server().await;
    let api_key = bootstrap_api_key(&url, &client, "icon_reject").await;
    let topic_id = create_topic(&url, &client, "python", &api_key).await;

    let bad = client
        .post(format!("{url}/app/topics/set-icon"))
        .json(&serde_json::json!({ "id": topic_id, "icon_id": "lucide:not-a-real-icon" }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), reqwest::StatusCode::BAD_REQUEST);

    let empty = client
        .post(format!("{url}/app/topics/set-icon"))
        .json(&serde_json::json!({ "id": topic_id, "icon_id": "  " }))
        .send()
        .await
        .unwrap();
    assert_eq!(empty.status(), reqwest::StatusCode::BAD_REQUEST);
}
