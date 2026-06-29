use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};

use super::client::JiraClient;
use super::models::{ApiField, FieldChoice, SearchResponse};

/// Discover the Jira "Story Points" customfield id for this instance.
///
/// Tier 1: exact name "Story Points"            (classic projects)
/// Tier 2: exact name "Story point estimate"    (next-gen / team-managed)
/// Tier 3: case-insensitive contains "story point"
///
/// When multiple fields share the same tier-match, we probe the Jira search
/// API to find which one actually carries non-null data for recent tickets
/// (prevents the "wrong field → 0 points" problem on instances that expose
/// both classic and next-gen story-point fields simultaneously).
///
/// On no match, returns `NoStoryPointsField` with a candidate list of
/// custom fields whose name suggests an estimate, so the user can pick.
pub async fn discover_story_points(client: &JiraClient) -> AppResult<String> {
    let fields: Vec<ApiField> = client.get_json("/field").await?;

    // Tier 1: exact "Story Points"
    let tier1: Vec<_> = fields.iter().filter(|f| f.name == "Story Points").collect();
    if tier1.len() == 1 {
        return Ok(tier1[0].id.clone());
    }
    if tier1.len() > 1 {
        if let Some(id) = probe_best(client, &tier1).await {
            return Ok(id);
        }
        let candidates = tier1.iter()
            .map(|f| FieldChoice { id: f.id.clone(), name: f.name.clone() })
            .collect();
        return Err(AppError::NoStoryPointsField(candidates));
    }

    // Tier 2: exact "Story point estimate"
    let tier2: Vec<_> = fields.iter().filter(|f| f.name == "Story point estimate").collect();
    if tier2.len() == 1 {
        return Ok(tier2[0].id.clone());
    }
    if tier2.len() > 1 {
        if let Some(id) = probe_best(client, &tier2).await {
            return Ok(id);
        }
        let candidates = tier2.iter()
            .map(|f| FieldChoice { id: f.id.clone(), name: f.name.clone() })
            .collect();
        return Err(AppError::NoStoryPointsField(candidates));
    }

    // Tier 3: any field whose name contains "story point"
    let tier3: Vec<_> = fields.iter()
        .filter(|f| f.name.to_ascii_lowercase().contains("story point"))
        .collect();
    if !tier3.is_empty() {
        if tier3.len() == 1 {
            return Ok(tier3[0].id.clone());
        }
        if let Some(id) = probe_best(client, &tier3).await {
            return Ok(id);
        }
        // Fall through to surface as candidates
    }

    // No match — surface all estimation-ish custom fields as candidates
    let candidates: Vec<FieldChoice> = fields
        .iter()
        .filter(|f| {
            let n = f.name.to_ascii_lowercase();
            f.id.starts_with("customfield_")
                && (n.contains("point") || n.contains("estimate") || n.contains("size"))
        })
        .map(|f| FieldChoice { id: f.id.clone(), name: f.name.clone() })
        .collect();
    Err(AppError::NoStoryPointsField(candidates))
}

#[derive(Serialize)]
struct ProbeBody<'a> {
    jql: &'a str,
    fields: Vec<&'a str>,
    #[serde(rename = "maxResults")]
    max_results: usize,
}

/// Try fetching 3 recently-done issues for each candidate field.
/// Returns the first field id that has a non-null numeric value.
async fn probe_best(client: &JiraClient, candidates: &[&ApiField]) -> Option<String> {
    let jql = "assignee = currentUser() AND statusCategory = Done ORDER BY updated DESC";
    for f in candidates {
        let body = ProbeBody {
            jql,
            fields: vec!["summary", &f.id],
            max_results: 3,
        };
        if let Ok(resp) = client.post_json::<SearchResponse, ProbeBody>("/search", &body).await {
            let has_data = resp.issues.iter().any(|issue| {
                issue.fields.get(&f.id)
                    .and_then(Value::as_f64)
                    .map(|v| v > 0.0)
                    .unwrap_or(false)
            });
            if has_data {
                return Some(f.id.clone());
            }
        }
    }
    None
}

// keep the pure helper for unit tests
pub fn pick_story_points(fields: &[ApiField]) -> AppResult<String> {
    let tier1: Vec<_> = fields.iter().filter(|f| f.name == "Story Points").collect();
    if tier1.len() == 1 { return Ok(tier1[0].id.clone()); }
    if tier1.len() > 1 {
        let c = tier1.iter().map(|f| FieldChoice { id: f.id.clone(), name: f.name.clone() }).collect();
        return Err(AppError::NoStoryPointsField(c));
    }
    let tier2: Vec<_> = fields.iter().filter(|f| f.name == "Story point estimate").collect();
    if tier2.len() == 1 { return Ok(tier2[0].id.clone()); }
    if tier2.len() > 1 {
        let c = tier2.iter().map(|f| FieldChoice { id: f.id.clone(), name: f.name.clone() }).collect();
        return Err(AppError::NoStoryPointsField(c));
    }
    if let Some(f) = fields.iter().find(|f| f.name.to_ascii_lowercase().contains("story point")) {
        return Ok(f.id.clone());
    }
    let candidates: Vec<FieldChoice> = fields.iter()
        .filter(|f| {
            let n = f.name.to_ascii_lowercase();
            f.id.starts_with("customfield_") && (n.contains("point") || n.contains("estimate") || n.contains("size"))
        })
        .map(|f| FieldChoice { id: f.id.clone(), name: f.name.clone() })
        .collect();
    Err(AppError::NoStoryPointsField(candidates))
}

pub async fn list_candidates(client: &JiraClient) -> AppResult<Vec<FieldChoice>> {
    let fields: Vec<ApiField> = client.get_json("/field").await?;
    Ok(fields
        .into_iter()
        .filter(|f| f.id.starts_with("customfield_"))
        .map(|f| FieldChoice { id: f.id, name: f.name })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fld(id: &str, name: &str) -> ApiField {
        ApiField { id: id.into(), name: name.into() }
    }

    #[test]
    fn picks_story_points_first() {
        let v = vec![
            fld("customfield_10016", "Story point estimate"),
            fld("customfield_10004", "Story Points"),
        ];
        assert_eq!(pick_story_points(&v).unwrap(), "customfield_10004");
    }

    #[test]
    fn picks_estimate_second() {
        let v = vec![
            fld("customfield_10016", "Story point estimate"),
            fld("summary", "Summary"),
        ];
        assert_eq!(pick_story_points(&v).unwrap(), "customfield_10016");
    }

    #[test]
    fn picks_contains_third() {
        let v = vec![fld("customfield_99999", "Original story point spike")];
        assert_eq!(pick_story_points(&v).unwrap(), "customfield_99999");
    }

    #[test]
    fn no_match_returns_candidates() {
        let v = vec![
            fld("customfield_1", "T-shirt size"),
            fld("customfield_2", "Estimate hours"),
            fld("summary", "Summary"),
        ];
        let err = pick_story_points(&v).unwrap_err();
        match err {
            AppError::NoStoryPointsField(c) => {
                assert_eq!(c.len(), 2);
                assert!(c.iter().any(|x| x.id == "customfield_1"));
                assert!(c.iter().any(|x| x.id == "customfield_2"));
            }
            _ => panic!("wrong error"),
        }
    }
}
