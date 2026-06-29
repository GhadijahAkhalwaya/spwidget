use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChoice {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueSummary {
    pub key: String,
    pub summary: String,
    pub points: f64,
    pub resolved: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub total_points: f64,
    pub issue_count: usize,
    pub issues: Vec<IssueSummary>,
    pub fetched_at: DateTime<Utc>,
}

// Minimal Jira API response shapes — only the fields we read.

#[derive(Debug, Deserialize)]
pub struct ApiField {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    #[serde(default)]
    pub total: usize,
    pub issues: Vec<ApiIssue>,
    #[serde(default, rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    #[serde(default, rename = "isLast")]
    pub is_last: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ApiIssue {
    pub key: String,
    pub fields: serde_json::Value,
}
