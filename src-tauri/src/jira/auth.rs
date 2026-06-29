use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, ACCEPT};

use super::flavor::Flavor;

pub fn build_headers(flavor: Flavor, user: &str, token: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    let value = match flavor {
        Flavor::Cloud => {
            let encoded = STANDARD.encode(format!("{user}:{token}"));
            format!("Basic {encoded}")
        }
        Flavor::Server => format!("Bearer {token}"),
    };
    if let Ok(v) = HeaderValue::from_str(&value) {
        h.insert(AUTHORIZATION, v);
    }
    h.insert(ACCEPT, HeaderValue::from_static("application/json"));
    h
}
