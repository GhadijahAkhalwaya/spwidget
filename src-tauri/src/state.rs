use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::Mutex;

use crate::cache;
use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::jira::client::JiraClient;
use crate::jira::flavor::normalize;
use crate::jira::models::Snapshot;
use crate::jira::search::fetch_snapshot;
use crate::secrets;

pub struct AppContext {
    pub app_data_dir: PathBuf,
    pub config: Option<Config>,
    pub cache: Option<Snapshot>,
}

impl AppContext {
    pub fn load(app_data_dir: PathBuf) -> AppResult<Self> {
        let config = Config::load(&app_data_dir)?;
        let cache = cache::load(&app_data_dir).unwrap_or(None);
        Ok(Self { app_data_dir, config, cache })
    }

    pub fn build_client(&self) -> AppResult<JiraClient> {
        let cfg = self.config.as_ref().ok_or(AppError::NotConfigured)?;
        let token = secrets::load_token(&self.app_data_dir)?.ok_or(AppError::NotConfigured)?;
        let (base, _, _) = normalize(&cfg.base_url)?;
        JiraClient::new(base, cfg.flavor, cfg.user.clone(), token)
    }
}

pub async fn refresh_locked(ctx: &Arc<Mutex<AppContext>>) -> AppResult<Snapshot> {
    let (client, sp_field, app_dir, mode, project_key) = {
        let guard = ctx.lock().await;
        let cfg = guard.config.as_ref().ok_or(AppError::NotConfigured)?;
        let client = guard.build_client()?;
        (
            client,
            cfg.story_points_field.clone(),
            guard.app_data_dir.clone(),
            cfg.mode.clone(),
            cfg.project_key.clone(),
        )
    };

    let snap = fetch_snapshot(&client, &sp_field, &mode, project_key.as_deref()).await?;

    {
        let mut guard = ctx.lock().await;
        if let Some(cfg) = guard.config.as_mut() {
            cfg.last_refresh = Some(Utc::now());
            let _ = cfg.save(&app_dir);
        }
        let _ = cache::save(&app_dir, &snap);
        guard.cache = Some(snap.clone());
    }
    Ok(snap)
}
