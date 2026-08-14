use std::{sync::Arc, time::Duration};

use crate::{AppState, services::image_enrichment::ImageJobRun};

pub fn spawn(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            let worked = run_once(&state).await;
            if !worked {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    });
}

/// Process at most one durable job. Kept callable so PostgreSQL integration
/// tests can prove lease, retry, and attachment behavior without a timer loop.
pub async fn run_once(state: &AppState) -> bool {
    match crate::services::image_enrichment::process_one(state).await {
        Ok(ImageJobRun::Idle) => false,
        Ok(ImageJobRun::Attached(card_id)) => {
            tracing::info!(card_id, "image enrichment job completed");
            true
        }
        Ok(ImageJobRun::CompletedWithoutImage(card_id)) => {
            tracing::info!(card_id, "image enrichment job completed without an image");
            true
        }
        Ok(ImageJobRun::Retrying(_)) | Ok(ImageJobRun::Failed(_)) => true,
        Err(error) => {
            tracing::error!(error = ?error, "image enrichment worker failed");
            false
        }
    }
}
