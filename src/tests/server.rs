use super::support::spawn_test_server;

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
    assert!(body.contains("Denpie"));
    assert!(body.contains("/_astro/"));
}

#[tokio::test]
async fn test_tipcard_image_append_requires_session_auth() {
    let (url, _client) = spawn_test_server().await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{url}/app/tipcard-images/append"))
        .json(&serde_json::json!({
            "card_id": 1,
            "image_data": [],
            "pool_image_ids": [],
            "urls": []
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}
