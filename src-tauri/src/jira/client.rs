use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::de::DeserializeOwned;
use url::Url;

use super::auth::build_headers;
use super::flavor::{join, Flavor};
use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub struct JiraClient {
    pub base: Url,
    pub flavor: Flavor,
    pub user: String,
    pub token: String,
    http: Client,
}

impl JiraClient {
    pub fn new(base: Url, flavor: Flavor, user: String, token: String) -> AppResult<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(15))
            .gzip(true)
            .user_agent("jira-points-widget/0.1")
            .build()
            .map_err(|e| AppError::Network(e.to_string()))?;
        Ok(Self {
            base,
            flavor,
            user,
            token,
            http,
        })
    }

    pub fn api_path(&self, suffix: &str) -> AppResult<Url> {
        let api_root = self.flavor.api_root();
        join(&self.base, &format!("{api_root}{suffix}"))
    }

    fn check_status(status: StatusCode) -> AppResult<()> {
        if status.is_success() {
            Ok(())
        } else if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
            Err(AppError::Auth)
        } else {
            Err(AppError::Network(format!("HTTP {status}")))
        }
    }

    pub async fn get_json<T: DeserializeOwned>(&self, suffix: &str) -> AppResult<T> {
        let url = self.api_path(suffix)?;
        let headers = build_headers(self.flavor, &self.user, &self.token);
        let resp = self.http.get(url).headers(headers).send().await?;
        Self::check_status(resp.status())?;
        Ok(resp.json::<T>().await?)
    }

    pub async fn post_json<T: DeserializeOwned, B: serde::Serialize>(
        &self,
        suffix: &str,
        body: &B,
    ) -> AppResult<T> {
        let url = self.api_path(suffix)?;
        let headers = build_headers(self.flavor, &self.user, &self.token);
        let resp = self
            .http
            .post(url)
            .headers(headers)
            .json(body)
            .send()
            .await?;
        Self::check_status(resp.status())?;
        Ok(resp.json::<T>().await?)
    }
}
