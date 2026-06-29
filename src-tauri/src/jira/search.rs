use chrono::Utc;
use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};

use super::client::JiraClient;
use super::flavor::Flavor;
use super::models::{ApiIssue, IssueSummary, SearchResponse, Snapshot};

const PAGE: usize = 100;
const SAFETY_CAP: usize = 5_000;

#[derive(Serialize)]
struct ClassicSearchBody<'a> {
    jql: &'a str,
    fields: Vec<&'a str>,
    #[serde(rename = "maxResults")]
    max_results: usize,
    #[serde(rename = "startAt")]
    start_at: usize,
}

#[derive(Serialize)]
struct EnhancedSearchBody<'a> {
    jql: &'a str,
    fields: Vec<&'a str>,
    #[serde(rename = "maxResults")]
    max_results: usize,
    #[serde(rename = "nextPageToken", skip_serializing_if = "Option::is_none")]
    next_page_token: Option<&'a str>,
}

/// Build the JQL query for the snapshot.
///
/// Uses `statusCategoryChangedDate` — the timestamp of the last status-category
/// transition — NOT `updated`, which is reset by any non-status change (comment,
/// label, watcher, etc.) and would cause stale Done issues to drift back into
/// the window every time someone touches them.
///
/// `project_key` (when supplied) scopes the count to a single project so issues
/// completed in unrelated projects don't pollute the tally.
fn build_jql(mode: &str, project_key: Option<&str>) -> String {
    let mut clauses: Vec<String> = Vec::with_capacity(4);
    clauses.push("assignee = currentUser()".to_string());
    if let Some(p) = project_key {
        let p = p.trim();
        if !p.is_empty() {
            // Quote the project key to handle any unusual characters.
            clauses.push(format!("project = \"{}\"", p.replace('"', "\\\"")));
        }
    }
    clauses.push("statusCategory = Done".to_string());

    if mode == "monthly" {
        use chrono::Datelike;
        let now = Utc::now();
        clauses.push(format!(
            "statusCategoryChangedDate >= \"{}-{:02}-01\"",
            now.year(),
            now.month()
        ));
    } else {
        clauses.push("statusCategoryChangedDate >= -90d".to_string());
    }

    // ORDER BY makes pagination deterministic — without it, two pages can
    // return overlapping or skipped issues if the server uses a non-stable sort.
    format!(
        "{} ORDER BY statusCategoryChangedDate DESC",
        clauses.join(" AND ")
    )
}

pub async fn fetch_snapshot(
    client: &JiraClient,
    sp_field: &str,
    mode: &str,
    project_key: Option<&str>,
) -> AppResult<Snapshot> {
    let jql = build_jql(mode, project_key);
    let mut total_points: f64 = 0.0;
    let mut summaries: Vec<IssueSummary> = Vec::new();

    let fields: Vec<&str> = vec![
        "summary",
        "resolutiondate",
        "statusCategoryChangedDate",
        sp_field,
    ];

    match client.flavor {
        Flavor::Cloud => {
            // Cloud: prefer enhanced search /search/jql with cursor pagination,
            // fall back to classic /search if the new endpoint isn't available.
            if !try_enhanced(client, &jql, &fields, sp_field, &mut total_points, &mut summaries)
                .await?
            {
                run_classic(client, &jql, &fields, sp_field, &mut total_points, &mut summaries)
                    .await?;
            }
        }
        Flavor::Server => {
            run_classic(client, &jql, &fields, sp_field, &mut total_points, &mut summaries).await?;
        }
    }

    Ok(Snapshot {
        total_points: round1(total_points),
        issue_count: summaries.len(),
        issues: summaries,
        fetched_at: Utc::now(),
    })
}

/// Returns Ok(true) if enhanced search succeeded; Ok(false) if the endpoint
/// returned 404/410 (use classic). Other errors propagate.
async fn try_enhanced(
    client: &JiraClient,
    jql: &str,
    fields: &[&str],
    sp_field: &str,
    total_points: &mut f64,
    summaries: &mut Vec<IssueSummary>,
) -> AppResult<bool> {
    let mut next: Option<String> = None;
    let mut seen = 0usize;
    loop {
        let body = EnhancedSearchBody {
            jql,
            fields: fields.to_vec(),
            max_results: PAGE,
            next_page_token: next.as_deref(),
        };
        let resp: Result<SearchResponse, AppError> =
            client.post_json("/search/jql", &body).await;
        let page = match resp {
            Ok(r) => r,
            Err(AppError::Network(msg)) if msg.contains("404") || msg.contains("410") => {
                return Ok(false);
            }
            Err(e) => return Err(e),
        };
        sum_page(&page.issues, sp_field, total_points, summaries)?;
        seen += page.issues.len();
        if seen >= SAFETY_CAP {
            break;
        }
        if page.is_last == Some(true) || page.next_page_token.is_none() {
            break;
        }
        next = page.next_page_token;
    }
    Ok(true)
}

async fn run_classic(
    client: &JiraClient,
    jql: &str,
    fields: &[&str],
    sp_field: &str,
    total_points: &mut f64,
    summaries: &mut Vec<IssueSummary>,
) -> AppResult<()> {
    let mut start_at = 0usize;
    loop {
        let body = ClassicSearchBody {
            jql,
            fields: fields.to_vec(),
            max_results: PAGE,
            start_at,
        };
        let page: SearchResponse = client.post_json("/search", &body).await?;
        if page.issues.is_empty() {
            break;
        }
        sum_page(&page.issues, sp_field, total_points, summaries)?;
        start_at += page.issues.len();
        if start_at >= page.total || start_at >= SAFETY_CAP {
            break;
        }
    }
    Ok(())
}

fn sum_page(
    issues: &[ApiIssue],
    sp_field: &str,
    total_points: &mut f64,
    summaries: &mut Vec<IssueSummary>,
) -> AppResult<()> {
    for issue in issues {
        let pts = extract_points(&issue.fields, sp_field);
        let summary = issue
            .fields
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        // Prefer the actual status-category transition timestamp.
        // Fall back to resolutiondate (only if the workflow populates it).
        let resolved = issue
            .fields
            .get("statusCategoryChangedDate")
            .and_then(Value::as_str)
            .or_else(|| issue.fields.get("resolutiondate").and_then(Value::as_str))
            .unwrap_or("")
            .to_string();
        *total_points += pts;
        summaries.push(IssueSummary {
            key: issue.key.clone(),
            summary,
            points: pts,
            resolved,
        });
    }
    Ok(())
}

pub fn extract_points(fields: &Value, sp_field: &str) -> f64 {
    fields
        .get(sp_field)
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

pub fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_handles_null_int_float_missing() {
        let f = "customfield_10016";

        let v = json!({ "customfield_10016": 5 });
        assert_eq!(extract_points(&v, f), 5.0);

        let v = json!({ "customfield_10016": 2.5 });
        assert_eq!(extract_points(&v, f), 2.5);

        let v = json!({ "customfield_10016": null });
        assert_eq!(extract_points(&v, f), 0.0);

        let v = json!({ "summary": "x" });
        assert_eq!(extract_points(&v, f), 0.0);
    }

    #[test]
    fn sum_page_correct() {
        let f = "customfield_10016";
        let issues = vec![
            ApiIssue {
                key: "A-1".into(),
                fields: json!({"customfield_10016": 3, "summary": "a"}),
            },
            ApiIssue {
                key: "A-2".into(),
                fields: json!({"customfield_10016": 1.5, "summary": "b"}),
            },
            ApiIssue {
                key: "A-3".into(),
                fields: json!({"customfield_10016": null, "summary": "c"}),
            },
        ];
        let mut total = 0.0;
        let mut out = Vec::new();
        sum_page(&issues, f, &mut total, &mut out).unwrap();
        assert_eq!(round1(total), 4.5);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].points, 3.0);
        assert_eq!(out[2].points, 0.0);
    }

    #[test]
    fn round1_rounds() {
        assert_eq!(round1(4.55), 4.6);
        assert_eq!(round1(4.54), 4.5);
        assert_eq!(round1(0.0), 0.0);
    }
}
