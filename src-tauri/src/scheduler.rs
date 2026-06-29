use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tokio::time::{interval, MissedTickBehavior};

use crate::error::AppError;
use crate::state::{refresh_locked, AppContext};

const DAY: Duration = Duration::from_secs(24 * 3600);

pub fn spawn_daily_refresh(handle: AppHandle, ctx: Arc<Mutex<AppContext>>) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = interval(DAY);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await; // first tick fires immediately
            match refresh_locked(&ctx).await {
                Ok(snap) => {
                    let _ = handle.emit("points-updated", &snap);
                }
                Err(AppError::NotConfigured) => {
                    // Setup not done yet — quiet skip.
                }
                Err(AppError::Auth) => {
                    let _ = handle.emit("auth-expired", ());
                    let _ = handle.emit(
                        "refresh-failed",
                        serde_json::json!({ "reason": "auth expired" }),
                    );
                }
                Err(e) => {
                    let _ = handle.emit(
                        "refresh-failed",
                        serde_json::json!({ "reason": e.to_string() }),
                    );
                }
            }
        }
    });
}
