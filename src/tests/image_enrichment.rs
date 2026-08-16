use base64::{Engine, engine::general_purpose::STANDARD};
use prost::Message;
use std::sync::Arc;

use crate::{
    api::pb,
    db::repositories::{image_pool, tipcards},
    tests::support::{
        TEST_USER_ID, bootstrap_api_key, make_state, post_api_v1_with_idempotency, setup_db,
        spawn_test_server_with_state, unique_settings_path,
    },
};

const ONE_PIXEL_PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

#[tokio::test]
async fn durable_image_job_attaches_once_and_recovers_after_a_lease_retry() {
    let pool = setup_db().await;
    let state = Arc::new(make_state(pool.clone(), unique_settings_path()));
    tokio::fs::create_dir_all(&state.image_dir).await.unwrap();

    let topic_id: i64 = sqlx::query_scalar(
        "INSERT INTO topics (user_id, name, tipcard_type, image_strategy)
         VALUES ($1, 'image jobs', 'repeatable_tip', 'pool') RETURNING id",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&pool)
    .await
    .unwrap();

    let pool_path = "worker-pool-source.png";
    let pool_bytes = STANDARD.decode(ONE_PIXEL_PNG).unwrap();
    tokio::fs::write(state.image_dir.join(pool_path), &pool_bytes)
        .await
        .unwrap();
    image_pool::insert_pool_image(
        &pool,
        TEST_USER_ID,
        pool_path,
        "image/png",
        pool_bytes.len() as i64,
        "Only image",
        Some("Deterministic worker fixture"),
        "[]",
    )
    .await
    .unwrap();

    let card_id = tipcards::create_generated_with_status(
        &pool,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "Illustrated card",
        "Full illustrated content",
        "Compact illustrated content",
        true,
        "one pixel learning diagram",
        "pending",
    )
    .await
    .unwrap();

    let queued: (String, i64) =
        sqlx::query_as("SELECT status, attempts FROM card_image_jobs WHERE card_id = $1")
            .bind(card_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(queued, ("pending".to_string(), 0));

    assert!(crate::image_enrichment::run_once(&state).await);
    let attached = tipcards::list_images(&pool, TEST_USER_ID, card_id)
        .await
        .unwrap();
    assert_eq!(attached.len(), 1);
    assert_eq!(attached[0].mime_type, "image/png");
    assert_eq!(attached[0].byte_size, pool_bytes.len() as i64);
    assert!(
        tokio::fs::try_exists(state.image_dir.join(&attached[0].storage_path))
            .await
            .unwrap()
    );
    let completed: String =
        sqlx::query_scalar("SELECT status FROM card_image_jobs WHERE card_id = $1")
            .bind(card_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(completed, "completed");

    let inventory = crate::api::list_tipcards_pb(&state, TEST_USER_ID)
        .await
        .unwrap();
    let pending = inventory
        .cards
        .iter()
        .find(|card| card.id == card_id)
        .expect("pending card must remain available in inventory");
    assert_eq!(pending.status, "pending");
    assert_eq!(pending.images.len(), 1);
    assert_eq!(pending.images[0].id, attached[0].id);
    assert_eq!(
        pending.images[0].download_path,
        format!("/api/v1/tipcard-images/{}", attached[0].id)
    );

    sqlx::query(
        "UPDATE card_image_jobs
         SET status = 'processing', lease_until = CURRENT_TIMESTAMP - INTERVAL '1 second'
         WHERE card_id = $1",
    )
    .bind(card_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(crate::image_enrichment::run_once(&state).await);
    let recovered = tipcards::list_images(&pool, TEST_USER_ID, card_id)
        .await
        .unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].id, attached[0].id);
    assert_eq!(recovered[0].storage_path, attached[0].storage_path);

    let promoted = tipcards::take_pending_card(&pool, TEST_USER_ID, topic_id, "repeatable_tip")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(promoted.id, card_id);
}

#[tokio::test]
async fn review_and_advance_returns_and_replays_the_promoted_card() {
    let (url, client, state) = spawn_test_server_with_state().await;
    let api_key = bootstrap_api_key(&url, &client, "review_advance").await;
    let topic_id: i64 = sqlx::query_scalar(
        "INSERT INTO topics (user_id, name, tipcard_type, daily_card_count)
         VALUES ($1, 'atomic flow', 'repeatable_tip', 3) RETURNING id",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&state.db)
    .await
    .unwrap();
    let active_id = tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "First",
        "First full",
        "First compact",
        false,
        "",
        "active",
    )
    .await
    .unwrap();
    let pending_id = tipcards::create_generated_with_status(
        &state.db,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "Second",
        "Second full",
        "Second compact",
        false,
        "",
        "pending",
    )
    .await
    .unwrap();
    tipcards::set_pinned(&state.db, TEST_USER_ID, active_id, true)
        .await
        .unwrap();

    let call = pb::ApiRequest {
        auth: String::new(),
        op: Some(pb::api_request::Op::ReviewAndAdvance(
            pb::ReviewAndAdvanceRequest {
                card_id: active_id,
                grade: 5,
                action: pb::ReviewActionValue::Learned as i32,
            },
        )),
    };
    let first = post_api_v1_with_idempotency(
        &url,
        &client,
        Some(&api_key),
        "review-advance-request-1",
        "review-advance-operation-1",
        call.clone(),
    )
    .await;
    assert_eq!(first.status(), reqwest::StatusCode::OK);
    assert!(first.headers().get("idempotency-replayed").is_none());
    let first = pb::ApiV1Response::decode(first.bytes().await.unwrap()).unwrap();
    let first = match first.outcome.unwrap() {
        pb::api_v1_response::Outcome::Success(response) => match response.result.unwrap() {
            pb::api_response::Result::ReviewAndAdvance(result) => result,
            other => panic!("unexpected result: {other:?}"),
        },
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(first.reviewed_card_id, active_id);
    assert_eq!(
        first.next_card.as_ref().map(|card| card.id),
        Some(pending_id)
    );
    assert!(
        first.next_card.as_ref().is_some_and(|card| card.pinned),
        "the repeatable topic slot must remain pinned after promotion"
    );
    assert!(!first.daily_complete);
    assert_eq!(first.pending_count, 0);

    let replay = post_api_v1_with_idempotency(
        &url,
        &client,
        Some(&api_key),
        "review-advance-request-2",
        "review-advance-operation-1",
        call,
    )
    .await;
    assert_eq!(replay.status(), reqwest::StatusCode::OK);
    assert_eq!(replay.headers()["idempotency-replayed"], "true");
    let replay = pb::ApiV1Response::decode(replay.bytes().await.unwrap()).unwrap();
    let replay = match replay.outcome.unwrap() {
        pb::api_v1_response::Outcome::Success(response) => match response.result.unwrap() {
            pb::api_response::Result::ReviewAndAdvance(result) => result,
            other => panic!("unexpected replay result: {other:?}"),
        },
        other => panic!("unexpected replay outcome: {other:?}"),
    };
    assert_eq!(
        replay.next_card.as_ref().map(|card| card.id),
        Some(pending_id)
    );

    let repeats: i64 = sqlx::query_scalar("SELECT repeats FROM review_states WHERE card_id = $1")
        .bind(active_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(repeats, 1, "idempotency replay must not apply review twice");
    let pin_rows = sqlx::query_as::<_, (i64, i64)>(
        "SELECT id, pinned FROM tipcards WHERE id IN ($1, $2) ORDER BY id",
    )
    .bind(active_id)
    .bind(pending_id)
    .fetch_all(&state.db)
    .await
    .unwrap();
    assert_eq!(
        pin_rows
            .iter()
            .find(|(id, _)| *id == active_id)
            .map(|(_, pinned)| *pinned),
        Some(0)
    );
    assert_eq!(
        pin_rows
            .iter()
            .find(|(id, _)| *id == pending_id)
            .map(|(_, pinned)| *pinned),
        Some(1)
    );

    let missing_call = pb::ApiRequest {
        auth: String::new(),
        op: Some(pb::api_request::Op::ReviewAndAdvance(
            pb::ReviewAndAdvanceRequest {
                card_id: i64::MAX,
                grade: 5,
                action: pb::ReviewActionValue::Learned as i32,
            },
        )),
    };
    let unauthenticated_client = reqwest::Client::new();
    let unauthenticated = post_api_v1_with_idempotency(
        &url,
        &unauthenticated_client,
        None,
        "review-advance-unauthenticated",
        "review-advance-unauthenticated",
        missing_call.clone(),
    )
    .await;
    assert_eq!(unauthenticated.status(), reqwest::StatusCode::UNAUTHORIZED);

    let missing = post_api_v1_with_idempotency(
        &url,
        &client,
        Some(&api_key),
        "review-advance-missing",
        "review-advance-missing",
        missing_call.clone(),
    )
    .await;
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);

    let create_read_key = post_api_v1_with_idempotency(
        &url,
        &client,
        Some(&api_key),
        "review-advance-read-key",
        "review-advance-read-key",
        pb::ApiRequest {
            auth: String::new(),
            op: Some(pb::api_request::Op::CreateApiKeyV1(
                pb::CreateApiKeyV1Request {
                    client_name: "review advance scope denial".to_string(),
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
    let forbidden = post_api_v1_with_idempotency(
        &url,
        &client,
        Some(&read_key),
        "review-advance-forbidden",
        "review-advance-forbidden",
        missing_call,
    )
    .await;
    assert_eq!(forbidden.status(), reqwest::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn missing_image_completes_without_retry_and_can_be_requeued() {
    let pool = setup_db().await;
    let state = Arc::new(make_state(pool.clone(), unique_settings_path()));
    tokio::fs::create_dir_all(&state.image_dir).await.unwrap();

    let topic_id: i64 = sqlx::query_scalar(
        "INSERT INTO topics (user_id, name, tipcard_type, image_strategy)
         VALUES ($1, 'empty pool', 'repeatable_tip', 'pool') RETURNING id",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    let card_id = tipcards::create_generated_with_status(
        &pool,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "Needs a picture",
        "Full content",
        "Compact content",
        true,
        "diagram that is not in the pool",
        "pending",
    )
    .await
    .unwrap();

    assert!(crate::image_enrichment::run_once(&state).await);
    let attached = tipcards::list_images(&pool, TEST_USER_ID, card_id)
        .await
        .unwrap();
    assert!(attached.is_empty());
    let status: String =
        sqlx::query_scalar("SELECT status FROM card_image_jobs WHERE card_id = $1")
            .bind(card_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "completed");

    let requeued =
        crate::db::repositories::image_jobs::requeue_failed_for_user(&pool, TEST_USER_ID)
            .await
            .unwrap();
    assert_eq!(requeued, 1);
    let pending: String =
        sqlx::query_scalar("SELECT status FROM card_image_jobs WHERE card_id = $1")
            .bind(card_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending, "pending");

    let mut tx = pool.begin().await.unwrap();
    crate::db::repositories::image_jobs::enqueue_in_tx(&mut tx, TEST_USER_ID, card_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let after_enqueue: (String, i64) =
        sqlx::query_as("SELECT status, attempts FROM card_image_jobs WHERE card_id = $1")
            .bind(card_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after_enqueue, ("pending".to_string(), 0));
}

#[tokio::test]
async fn enqueue_does_not_replace_an_already_attached_image() {
    let pool = setup_db().await;
    let state = Arc::new(make_state(pool.clone(), unique_settings_path()));
    tokio::fs::create_dir_all(&state.image_dir).await.unwrap();

    let topic_id: i64 = sqlx::query_scalar(
        "INSERT INTO topics (user_id, name, tipcard_type, image_strategy)
         VALUES ($1, 'keep image', 'repeatable_tip', 'pool') RETURNING id",
    )
    .bind(TEST_USER_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    let pool_path = "keep-pool-source.png";
    let pool_bytes = STANDARD.decode(ONE_PIXEL_PNG).unwrap();
    tokio::fs::write(state.image_dir.join(pool_path), &pool_bytes)
        .await
        .unwrap();
    image_pool::insert_pool_image(
        &pool,
        TEST_USER_ID,
        pool_path,
        "image/png",
        pool_bytes.len() as i64,
        "Only image",
        Some("Keep this attachment"),
        "[]",
    )
    .await
    .unwrap();
    let card_id = tipcards::create_generated_with_status(
        &pool,
        TEST_USER_ID,
        topic_id,
        "repeatable_tip",
        "Illustrated card",
        "Full illustrated content",
        "Compact illustrated content",
        true,
        "one pixel learning diagram",
        "pending",
    )
    .await
    .unwrap();
    assert!(crate::image_enrichment::run_once(&state).await);
    let attached = tipcards::list_images(&pool, TEST_USER_ID, card_id)
        .await
        .unwrap();
    assert_eq!(attached.len(), 1);
    let attached_id = attached[0].id;

    let requeued =
        crate::db::repositories::image_jobs::requeue_failed_for_user(&pool, TEST_USER_ID)
            .await
            .unwrap();
    assert_eq!(requeued, 0);

    let mut tx = pool.begin().await.unwrap();
    crate::db::repositories::image_jobs::enqueue_in_tx(&mut tx, TEST_USER_ID, card_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(crate::image_enrichment::run_once(&state).await);
    let still_attached = tipcards::list_images(&pool, TEST_USER_ID, card_id)
        .await
        .unwrap();
    assert_eq!(still_attached.len(), 1);
    assert_eq!(still_attached[0].id, attached_id);
}
