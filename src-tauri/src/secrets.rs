// Token storage: try macOS Keychain first; fall back to a chmod-600 file.
// Keychain can silently fail on unsigned dev binaries, so the file fallback
// ensures the app works during development. A signed .app bundle will use
// Keychain only (no file written).

use std::fs;
use std::path::{Path, PathBuf};

use keyring::Entry;

use crate::error::AppResult;

const SERVICE: &str = "com.local.jira-points-widget";
const ACCOUNT: &str = "jira-token";

fn token_file(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(".token")
}

fn keychain_entry() -> Option<Entry> {
    Entry::new(SERVICE, ACCOUNT).ok()
}

/// Save token: Keychain first, always mirror to file as fallback.
pub fn save_token(token: &str, app_data_dir: &Path) -> AppResult<()> {
    // Best-effort Keychain write (may fail on unsigned dev binaries).
    if let Some(e) = keychain_entry() {
        let _ = e.set_password(token);
    }

    // Reliable file fallback with restricted permissions.
    fs::create_dir_all(app_data_dir)?;
    let path = token_file(app_data_dir);
    fs::write(&path, token)?;

    // chmod 600 — readable only by the current user.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

/// Load token: Keychain first, fall back to file.
pub fn load_token(app_data_dir: &Path) -> AppResult<Option<String>> {
    if let Some(e) = keychain_entry() {
        match e.get_password() {
            Ok(s) if !s.is_empty() => return Ok(Some(s)),
            _ => {}
        }
    }
    let path = token_file(app_data_dir);
    if path.exists() {
        let s = fs::read_to_string(&path)?.trim().to_string();
        if !s.is_empty() {
            return Ok(Some(s));
        }
    }
    Ok(None)
}

/// Clear token from both stores.
pub fn clear_token(app_data_dir: &Path) -> AppResult<()> {
    if let Some(e) = keychain_entry() {
        let _ = e.delete_credential();
    }
    let path = token_file(app_data_dir);
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}
