use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SSEClientParams {
    pub ws_url: String,
    pub http_url: String,
}

impl Default for SSEClientParams {
    fn default() -> Self {
        Self {
            ws_url: "ws://localhost:8080".to_string(),
            http_url: "http://localhost:8080".to_string(),
        }
    }
}

impl SSEClientParams {
    pub fn validate(&self) -> Result<(), String> {
        if self.ws_url.is_empty() {
            return Err("ws_url is required".to_string());
        }

        if !Self::is_valid_url(&self.ws_url) {
            return Err(format!("ws_url is not a valid URL: {}", self.ws_url));
        }

        if self.http_url.is_empty() {
            return Err("http_url is required".to_string());
        }

        if !Self::is_valid_url(&self.http_url) {
            return Err(format!("http_url is not a valid URL: {}", self.http_url));
        }

        Ok(())
    }

    /// Validates if a given URL is a properly formed HTTP or HTTPS URL.
    /// Returns `true` if valid, `false` otherwise.
    pub fn is_valid_url(url: &str) -> bool {
        match Url::parse(url) {
            Ok(parsed_url) => {
                let valid_scheme = parsed_url.scheme() == "http"
                    || parsed_url.scheme() == "https"
                    || parsed_url.scheme() == "ws"
                    || parsed_url.scheme() == "wss";
                let valid_host = parsed_url.host_str().is_some();

                valid_scheme && valid_host
            }
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_url() {
        assert!(SSEClientParams::is_valid_url("ws://localhost:8080"));
        assert!(SSEClientParams::is_valid_url("http://localhost:8080"));
        assert!(SSEClientParams::is_valid_url("https://localhost:8080"));
        assert!(SSEClientParams::is_valid_url("wss://localhost:8080"));
        assert!(SSEClientParams::is_valid_url("ws://google.com"));
        assert!(SSEClientParams::is_valid_url("http://google.com"));
        assert!(SSEClientParams::is_valid_url("https://google.com"));
        assert!(SSEClientParams::is_valid_url("wss://google.com"));
        assert!(!SSEClientParams::is_valid_url("ws://"));
        assert!(!SSEClientParams::is_valid_url("http://"));
        assert!(!SSEClientParams::is_valid_url("https://"));
        assert!(!SSEClientParams::is_valid_url("wss://"));
    }
}
