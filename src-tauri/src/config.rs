use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::jira::flavor::Flavor;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub base_url: String,
    pub flavor: Flavor,
    pub user: String,
    pub story_points_field: String,
    pub last_refresh: Option<DateTime<Utc>>,
    #[serde(default = "default_mode")]
    pub mode: String, // "days90" | "monthly"
    /// Optional Jira project key (e.g. "IRD"). When set, the tally is scoped
    /// to a single project so issues completed in unrelated projects don't
    /// pollute the count.
    #[serde(default)]
    pub project_key: Option<String>,
}

fn default_mode() -> String {
    "days90".to_string()
}

impl Config {
    pub fn path(app_data_dir: &Path) -> PathBuf {
        app_data_dir.join("config.json")
    }

    pub fn load(app_data_dir: &Path) -> AppResult<Option<Self>> {
        let path = Self::path(app_data_dir);
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)?;
        let cfg: Self = serde_json::from_str(&raw)?;
        Ok(Some(cfg))
    }

    pub fn save(&self, app_data_dir: &Path) -> AppResult<()> {
        fs::create_dir_all(app_data_dir)?;
        let path = Self::path(app_data_dir);
        let tmp = path.with_extension("json.tmp");
        let body = serde_json::to_string_pretty(self)?;
        fs::write(&tmp, body)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn delete(app_data_dir: &Path) -> AppResult<()> {
        let path = Self::path(app_data_dir);
        if path.exists() {
            fs::remove_file(&path).map_err(AppError::from)?;
        }
        Ok(())
    }
}
