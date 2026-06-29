use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{AppError, AppResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Flavor {
    Cloud,
    Server,
}

impl Flavor {
    pub fn detect(host: &str) -> Self {
        if host.to_ascii_lowercase().ends_with(".atlassian.net") {
            Flavor::Cloud
        } else {
            Flavor::Server
        }
    }

    pub fn api_root(self) -> &'static str {
        match self {
            Flavor::Cloud => "/rest/api/3",
            Flavor::Server => "/rest/api/2",
        }
    }
}

/// Normalize a user-entered base URL:
///   * require http(s) scheme
///   * trim trailing slash from path
///   * preserve any path prefix (e.g. https://jira.acme.com/jira)
/// Returns (normalized base URL, host string, flavor).
pub fn normalize(input: &str) -> AppResult<(Url, String, Flavor)> {
    let mut url = Url::parse(input.trim())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::Parse(format!(
            "URL scheme must be http or https (got {})",
            url.scheme()
        )));
    }
    let host = url
        .host_str()
        .ok_or_else(|| AppError::Parse("URL has no host".into()))?
        .to_string();

    // Strip trailing slash on path (but keep "/" if path is just "/").
    let path = url.path().trim_end_matches('/').to_string();
    let final_path = if path.is_empty() { "" } else { &path };
    url.set_path(final_path);
    url.set_query(None);
    url.set_fragment(None);

    let flavor = Flavor::detect(&host);
    Ok((url, host, flavor))
}

/// Join the normalized base URL with an api path like "/rest/api/3/myself".
pub fn join(base: &Url, suffix: &str) -> AppResult<Url> {
    let base_str = base.as_str().trim_end_matches('/');
    let combined = format!("{base_str}{suffix}");
    Url::parse(&combined).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_cloud() {
        assert_eq!(Flavor::detect("acme.atlassian.net"), Flavor::Cloud);
        assert_eq!(Flavor::detect("ACME.Atlassian.Net"), Flavor::Cloud);
    }

    #[test]
    fn detect_server() {
        assert_eq!(Flavor::detect("jira.acme.com"), Flavor::Server);
        assert_eq!(Flavor::detect("localhost"), Flavor::Server);
        assert_eq!(Flavor::detect("notatlassian.net"), Flavor::Server);
    }

    #[test]
    fn normalize_strips_trailing_slash() {
        let (u, host, f) = normalize("https://acme.atlassian.net/").unwrap();
        assert_eq!(u.as_str(), "https://acme.atlassian.net/");
        // url::Url always renders an empty path as "/", so check host & flavor
        assert_eq!(host, "acme.atlassian.net");
        assert_eq!(f, Flavor::Cloud);
    }

    #[test]
    fn normalize_preserves_prefix() {
        let (u, host, f) = normalize("https://jira.acme.com/jira/").unwrap();
        assert_eq!(u.as_str(), "https://jira.acme.com/jira");
        assert_eq!(host, "jira.acme.com");
        assert_eq!(f, Flavor::Server);
    }

    #[test]
    fn join_builds_correctly() {
        let (u, _, _) = normalize("https://jira.acme.com/jira").unwrap();
        let j = join(&u, "/rest/api/2/myself").unwrap();
        assert_eq!(j.as_str(), "https://jira.acme.com/jira/rest/api/2/myself");
    }

    #[test]
    fn rejects_non_http() {
        assert!(normalize("ftp://acme.atlassian.net").is_err());
    }
}
