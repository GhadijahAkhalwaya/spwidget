use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::jira::models::Snapshot;

pub fn cache_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("cache.json")
}

pub fn load(app_data_dir: &Path) -> AppResult<Option<Snapshot>> {
    let p = cache_path(app_data_dir);
    if !p.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&p)?;
    let snap = serde_json::from_str::<Snapshot>(&raw)?;
    Ok(Some(snap))
}

pub fn save(app_data_dir: &Path, snap: &Snapshot) -> AppResult<()> {
    fs::create_dir_all(app_data_dir)?;
    let p = cache_path(app_data_dir);
    let tmp = p.with_extension("json.tmp");
    let body = serde_json::to_string(snap)?;
    fs::write(&tmp, body)?;
    fs::rename(&tmp, &p)?;
    Ok(())
}

pub fn clear(app_data_dir: &Path) -> AppResult<()> {
    let p = cache_path(app_data_dir);
    if p.exists() {
        fs::remove_file(&p).map_err(AppError::from)?;
    }
    Ok(())
}
