//! Configuration management for the glim application.
//!
//! Centralizes all configuration options and provides a clean interface
//! for accessing application settings.

use std::net::{IpAddr, Ipv4Addr};

/// Application configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Server configuration
    pub server: ServerConfig,
    /// GitHub API configuration
    pub github: GitHubConfig,
    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,
}

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Default host address
    pub default_host: IpAddr,
    /// Default port
    pub default_port: u16,
    /// Health check token (optional)
    pub healthcheck_token: Option<String>,
    /// Hostname bypass for health checks (optional)
    pub healthcheck_host_bypass: Option<String>,
}

/// GitHub API configuration
#[derive(Debug, Clone)]
pub struct GitHubConfig {
    /// GitHub API token (optional)
    pub token: Option<String>,
    /// API retry attempts
    pub retry_attempts: u8,
}

/// Rate limiting configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Global requests per minute
    pub global_requests_per_minute: u32,
    /// Per-IP requests per minute
    pub per_ip_requests_per_minute: u32,
    /// IP memory duration in seconds
    pub ip_memory_duration: u64,
    /// Token refill interval in seconds
    pub refill_interval: u64,
}

/// CLI configuration overrides
#[derive(Debug, Clone)]
pub struct CliOverrides {
    /// GitHub token override
    pub token: Option<String>,
    /// Port override
    pub port: Option<u16>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            github: GitHubConfig::default(),
            rate_limit: RateLimitConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            default_host: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            default_port: 8080,
            healthcheck_token: None,
            healthcheck_host_bypass: None,
        }
    }
}

impl Default for GitHubConfig {
    fn default() -> Self {
        Self {
            token: None,
            retry_attempts: 3,
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            global_requests_per_minute: 300,
            per_ip_requests_per_minute: 30,
            ip_memory_duration: 3600,
            refill_interval: 1,
        }
    }
}

impl Config {
    /// Load configuration with CLI overrides
    pub fn load(cli_overrides: Option<CliOverrides>) -> Self {
        let mut config = Self::default();

        // Apply CLI overrides if provided
        if let Some(overrides) = cli_overrides {
            if let Some(token) = overrides.token {
                config.github.token = Some(token);
            }
            if let Some(port) = overrides.port {
                config.server.default_port = port;
            }
        }

        // Load from environment variables (CLI overrides take precedence)
        if config.github.token.is_none() {
            config.github.token = std::env::var("GITHUB_TOKEN").ok();
        }

        if config.server.default_port == 8080 {
            if let Ok(port_str) = std::env::var("PORT") {
                if let Ok(port) = port_str.parse::<u16>() {
                    config.server.default_port = port;
                }
            }
        }

        config.server.healthcheck_token = std::env::var("HEALTHCHECK_TOKEN").ok();
        config.server.healthcheck_host_bypass = std::env::var("HEALTHCHECK_HOST_BYPASS").ok();

        config
    }

    /// Get the default host address
    pub fn default_host(&self) -> IpAddr {
        self.server.default_host
    }

    /// Get the default port
    pub fn default_port(&self) -> u16 {
        self.server.default_port
    }

    /// Get the GitHub token
    pub fn github_token(&self) -> Option<&str> {
        self.github.token.as_deref()
    }

    /// Get the health check token
    pub fn healthcheck_token(&self) -> Option<&str> {
        self.server.healthcheck_token.as_deref()
    }

    /// Get the health check host bypass
    pub fn healthcheck_host_bypass(&self) -> Option<&str> {
        self.server.healthcheck_host_bypass.as_deref()
    }

    /// Get the rate limit configuration
    pub fn rate_limit_config(&self) -> &RateLimitConfig {
        &self.rate_limit
    }
}

impl CliOverrides {
    /// Create CLI overrides from CLI arguments
    pub fn from_cli_args(token: Option<String>, port: Option<u16>) -> Self {
        Self { token, port }
    }
}
