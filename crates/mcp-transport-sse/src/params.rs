use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SSEClientTransportParams {
    pub event_url: String,
    pub post_url: String,
}

impl Default for SSEClientTransportParams {
    fn default() -> Self {
        Self {
            event_url: "http://localhost:8080".to_string(),
            post_url: "http://localhost:8080".to_string(),
        }
    }
}

impl SSEClientTransportParams {
    pub fn validate(&self) -> Result<(), String> {
        if self.event_url.is_empty() {
            return Err("event_url is required".to_string());
        }

        if !Self::is_valid_url(&self.event_url) {
            return Err(format!("event_url is not a valid URL: {}", self.event_url));
        }

        if self.post_url.is_empty() {
            return Err("post_url is required".to_string());
        }

        if !Self::is_valid_url(&self.post_url) {
            return Err(format!("post_url is not a valid URL: {}", self.post_url));
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SSEServerTransportParams {
    pub listen_addr: String,
    pub ws_url: String,
}

impl Default for SSEServerTransportParams {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:8080".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        }
    }
}

impl SSEServerTransportParams {
    pub fn validate(&self) -> Result<(), String> {
        if self.listen_addr.is_empty() {
            return Err("listen_addr is required".to_string());
        }

        // Try to parse the address to validate it
        match self.listen_addr.parse::<std::net::SocketAddr>() {
            Ok(addr) => addr,
            Err(e) => return Err(format!("Invalid listen address: {}", e)),
        };

        // Validate ws_url
        if self.ws_url.is_empty() {
            return Err("ws_url is required".to_string());
        }

        if !SSEClientTransportParams::is_valid_url(&self.ws_url) {
            return Err(format!("ws_url is not a valid URL: {}", self.ws_url));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_url() {
        assert!(SSEClientTransportParams::is_valid_url(
            "ws://localhost:8080"
        ));
        assert!(SSEClientTransportParams::is_valid_url(
            "http://localhost:8080"
        ));
        assert!(SSEClientTransportParams::is_valid_url(
            "https://localhost:8080"
        ));
        assert!(SSEClientTransportParams::is_valid_url(
            "wss://localhost:8080"
        ));
        assert!(SSEClientTransportParams::is_valid_url("ws://google.com"));
        assert!(SSEClientTransportParams::is_valid_url("http://google.com"));
        assert!(SSEClientTransportParams::is_valid_url("https://google.com"));
        assert!(SSEClientTransportParams::is_valid_url("wss://google.com"));
        assert!(!SSEClientTransportParams::is_valid_url("ws://"));
        assert!(!SSEClientTransportParams::is_valid_url("http://"));
        assert!(!SSEClientTransportParams::is_valid_url("https://"));
        assert!(!SSEClientTransportParams::is_valid_url("wss://"));
    }

    #[test]
    fn test_server_transport_params_validation() {
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:8080".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };
        assert!(params.validate().is_ok());

        let params = SSEServerTransportParams {
            listen_addr: "".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };
        assert!(params.validate().is_err());

        let params = SSEServerTransportParams {
            listen_addr: "invalid".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_server_transport_params_port_validation() {
        // Test valid ports
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:8080".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };
        assert!(params.validate().is_ok());

        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:1".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };
        assert!(params.validate().is_ok());

        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:65535".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };
        assert!(params.validate().is_ok());

        // Test port 0 (valid for testing - system assigns random port)
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:0".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };
        assert!(params.validate().is_ok());

        // Test invalid ports
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:65536".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };
        assert!(params.validate().is_err());
        assert_eq!(
            params.validate().unwrap_err(),
            "Invalid listen address: invalid socket address syntax"
        );

        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:99999".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };
        assert!(params.validate().is_err());
        assert_eq!(
            params.validate().unwrap_err(),
            "Invalid listen address: invalid socket address syntax"
        );
    }

    #[test]
    fn test_server_transport_params_ws_url_validation() {
        // Test valid ws_url
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:8080".to_string(),
            ws_url: "ws://localhost:8080".to_string(),
        };
        assert!(params.validate().is_ok());

        // Test empty ws_url
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:8080".to_string(),
            ws_url: "".to_string(),
        };
        assert!(params.validate().is_err());
        assert_eq!(params.validate().unwrap_err(), "ws_url is required");

        // Test invalid ws_url
        let params = SSEServerTransportParams {
            listen_addr: "127.0.0.1:8080".to_string(),
            ws_url: "invalid".to_string(),
        };
        assert!(params.validate().is_err());
        assert_eq!(
            params.validate().unwrap_err(),
            "ws_url is not a valid URL: invalid"
        );
    }
}
