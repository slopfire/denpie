use std::{sync::Arc, time::Duration};

use crate::AppState;

pub fn spawn(state: Arc<AppState>) {
    tokio::spawn(async move {
        run_once(&state).await;
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            run_once(&state).await;
        }
    });
}

async fn run_once(state: &AppState) {
    match crate::api::refresh_due_daily_topics(state).await {
        Ok(0) => {}
        Ok(count) => eprintln!("daily refresh generated {count} card(s)"),
        Err((status, message)) => eprintln!("daily refresh failed ({status}): {message}"),
    }
}
