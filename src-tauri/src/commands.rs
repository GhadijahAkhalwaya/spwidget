use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::cache;
use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::jira::client::JiraClient;
use crate::jira::fields::{discover_story_points, list_candidates};
use crate::jira::flavor::normalize;
use crate::jira::models::{FieldChoice, Snapshot};
use crate::secrets;
use crate::state::{refresh_locked, AppContext};

pub type Ctx = Arc<Mutex<AppContext>>;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SetupResult {
    Ok,
    NeedsFieldPick { candidates: Vec<FieldChoice> },
}

#[derive(Serialize)]
pub struct PartialConfig {
    pub base_url: String,
    pub user: String,
}

#[tauri::command]
pub async fn is_configured(state: State<'_, Ctx>) -> AppResult<bool> {
    let g = state.lock().await;
    Ok(g.config.is_some()
        && secrets::load_token(&g.app_data_dir).ok().flatten().is_some())
}

#[tauri::command]
pub async fn get_config(state: State<'_, Ctx>) -> AppResult<Option<PartialConfig>> {
    let g = state.lock().await;
    Ok(g.config.as_ref().map(|c| PartialConfig {
        base_url: c.base_url.clone(),
        user: c.user.clone(),
    }))
}

#[tauri::command]
pub async fn get_points(state: State<'_, Ctx>) -> AppResult<Option<Snapshot>> {
    let g = state.lock().await;
    Ok(g.cache.clone())
}

#[tauri::command]
pub async fn refresh_now(state: State<'_, Ctx>) -> AppResult<Snapshot> {
    let arc = state.inner().clone();
    refresh_locked(&arc).await
}

#[tauri::command]
pub async fn save_credentials(
    app: AppHandle,
    state: State<'_, Ctx>,
    url: String,
    user: String,
    token: String,
    field_id: Option<String>,
) -> AppResult<SetupResult> {
    let (base, _host, flavor) = normalize(&url)?;
    let client = JiraClient::new(base.clone(), flavor, user.clone(), token.clone())?;

    // Auth probe
    let _: serde_json::Value = client.get_json("/myself").await?;

    // Resolve story-points field
    let sp_field = match field_id {
        Some(id) if !id.is_empty() => id,
        _ => match discover_story_points(&client).await {
            Ok(id) => id,
            Err(AppError::NoStoryPointsField(c)) => {
                return Ok(SetupResult::NeedsFieldPick { candidates: c });
            }
            Err(e) => return Err(e),
        },
    };

    // Preserve project_key from any existing config (re-setup with a fresh token
     // shouldn't wipe the user's project filter).
    let existing_project_key = {
        let g = state.lock().await;
        g.config.as_ref().and_then(|c| c.project_key.clone())
    };

    let cfg = Config {
        base_url: base.as_str().trim_end_matches('/').to_string(),
        flavor,
        user,
        story_points_field: sp_field,
        last_refresh: None,
        mode: "days90".to_string(),
        project_key: existing_project_key,
    };

    let app_dir = {
        let mut g = state.lock().await;
        let dir = g.app_data_dir.clone();
        cfg.save(&dir)?;
        g.config = Some(cfg);
        dir
    };
    secrets::save_token(&token, &app_dir)?;

    let _ = app.emit("configured", ());
    Ok(SetupResult::Ok)
}

#[tauri::command]
pub async fn list_point_candidates(state: State<'_, Ctx>) -> AppResult<Vec<FieldChoice>> {
    let client = {
        let g = state.lock().await;
        g.build_client()?
    };
    list_candidates(&client).await
}

#[tauri::command]
pub async fn get_mode(state: State<'_, Ctx>) -> AppResult<String> {
    let g = state.lock().await;
    Ok(g.config.as_ref()
        .map(|c| c.mode.clone())
        .unwrap_or_else(|| "days90".to_string()))
}

#[tauri::command]
pub async fn set_mode(state: State<'_, Ctx>, mode: String) -> AppResult<()> {
    let mut g = state.lock().await;
    let app_dir = g.app_data_dir.clone();
    let cfg = g.config.as_mut().ok_or(AppError::NotConfigured)?;
    cfg.mode = mode;
    cfg.save(&app_dir)
}

#[tauri::command]
pub async fn get_project_key(state: State<'_, Ctx>) -> AppResult<Option<String>> {
    let g = state.lock().await;
    Ok(g.config.as_ref().and_then(|c| c.project_key.clone()))
}

#[tauri::command]
pub async fn set_project_key(state: State<'_, Ctx>, project_key: Option<String>) -> AppResult<()> {
    let mut g = state.lock().await;
    let app_dir = g.app_data_dir.clone();
    let cfg = g.config.as_mut().ok_or(AppError::NotConfigured)?;
    // Empty string ⇒ clear filter (count across all projects)
    cfg.project_key = project_key
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    cfg.save(&app_dir)
}

#[tauri::command]
pub fn get_idle_seconds() -> f64 {
    use std::process::Command;
    let Ok(out) = Command::new("ioreg").args(["-c", "IOHIDSystem"]).output() else {
        return 0.0;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        if line.contains("HIDIdleTime") {
            if let Some(eq) = line.rfind('=') {
                let ns_str = line[eq + 1..].trim();
                if let Ok(ns) = ns_str.parse::<u64>() {
                    return ns as f64 / 1_000_000_000.0;
                }
            }
        }
    }
    0.0
}

#[tauri::command]
pub async fn clear_credentials(state: State<'_, Ctx>) -> AppResult<()> {
    let app_dir = {
        let mut g = state.lock().await;
        g.config = None;
        g.cache = None;
        g.app_data_dir.clone()
    };
    let _ = secrets::clear_token(&app_dir);
    let _ = Config::delete(&app_dir);
    let _ = cache::clear(&app_dir);
    Ok(())
}

#[tauri::command]
pub fn quit_app() {
    // Forcefully terminate the process. Used by the × button so that
    // closing the window quits the app entirely (it would otherwise keep
    // running invisibly thanks to ActivationPolicy::Accessory).
    std::process::exit(0);
}

// NSWindowCollectionBehaviorCanJoinAllSpaces = 1 << 0
// NSWindowCollectionBehaviorStationary       = 1 << 4
// Combined: window is visible on every Space and doesn't follow Space switches.
#[cfg(target_os = "macos")]
const COLLECTION_BEHAVIOR_ALL_SPACES_STATIONARY: u64 = (1 << 0) | (1 << 4);

/// Drop the widget below every normal application window.
/// Level -1 is below NSNormalWindowLevel (0), so no normal window can sit
/// behind it. Re-asserts the all-Spaces collection behavior so the window
/// doesn't get confined to the active Space when its level changes.
#[tauri::command]
pub fn send_to_back(window: tauri::WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        use objc::{msg_send, sel, sel_impl};
        use objc::runtime::Object;
        type Id = *mut Object;
        if let Ok(ns_win_ptr) = window.ns_window() {
            unsafe {
                let ns_win: Id = ns_win_ptr as Id;
                let _: () = msg_send![ns_win, setCollectionBehavior:
                    COLLECTION_BEHAVIOR_ALL_SPACES_STATIONARY];
                let _: () = msg_send![ns_win, setLevel: -1i64];
                let nil: Id = std::ptr::null_mut();
                let _: () = msg_send![ns_win, orderBack: nil];
            }
        }
    }
}

/// Bring the widget above every normal application window — on every Space.
/// NSFloatingWindowLevel = 3. Uses `orderFrontRegardless:` (not `orderFront:`)
/// which forces the window forward without activating the app, avoiding the
/// "warps to the active Space" behavior that `orderFront:` can trigger.
#[tauri::command]
pub fn send_to_front(window: tauri::WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        use objc::{msg_send, sel, sel_impl};
        use objc::runtime::Object;
        type Id = *mut Object;
        if let Ok(ns_win_ptr) = window.ns_window() {
            unsafe {
                let ns_win: Id = ns_win_ptr as Id;
                let _: () = msg_send![ns_win, setCollectionBehavior:
                    COLLECTION_BEHAVIOR_ALL_SPACES_STATIONARY];
                let _: () = msg_send![ns_win, setLevel: 3i64];
                let nil: Id = std::ptr::null_mut();
                let _: () = msg_send![ns_win, orderFrontRegardless];
                let _ = nil;
            }
        }
    }
}
