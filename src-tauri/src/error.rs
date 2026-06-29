use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::jira::models::FieldChoice;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("network error: {0}")]
    Network(String),

    #[error("authentication failed")]
    Auth,

    #[error("could not find a Story Points field; pick one manually")]
    NoStoryPointsField(Vec<FieldChoice>),

    #[error("keychain error: {0}")]
    Keychain(String),

    #[error("io error: {0}")]
    Io(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("not configured")]
    NotConfigured,
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Network(_) => "Network",
            AppError::Auth => "Auth",
            AppError::NoStoryPointsField(_) => "NoStoryPointsField",
            AppError::Keychain(_) => "Keychain",
            AppError::Io(_) => "Io",
            AppError::Parse(_) => "Parse",
            AppError::NotConfigured => "NotConfigured",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 3)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        if let AppError::NoStoryPointsField(c) = self {
            s.serialize_field("candidates", c)?;
        } else {
            s.serialize_field("candidates", &Option::<Vec<FieldChoice>>::None)?;
        }
        s.end()
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Network(e.to_string())
    }
}
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Parse(e.to_string())
    }
}
impl From<keyring::Error> for AppError {
    fn from(e: keyring::Error) -> Self {
        AppError::Keychain(e.to_string())
    }
}
impl From<url::ParseError> for AppError {
    fn from(e: url::ParseError) -> Self {
        AppError::Parse(format!("invalid URL: {e}"))
    }
}

pub type AppResult<T> = Result<T, AppError>;
