use glim::config::{CliOverrides, Config, GitHubConfig, RateLimitConfig, ServerConfig};
use std::net::{IpAddr, Ipv4Addr};

#[test]
fn test_default_config_values() {
    let config = Config::default();

    // Test server defaults
    assert_eq!(
        config.server.default_host,
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
    );
    assert_eq!(config.server.default_port, 8080);
    assert_eq!(config.server.healthcheck_token, None);
    assert_eq!(config.server.healthcheck_host_bypass, None);

    // Test GitHub defaults
    assert_eq!(config.github.token, None);
    assert_eq!(config.github.retry_attempts, 3);

    // Test rate limit defaults
    assert_eq!(config.rate_limit.global_requests_per_minute, 300);
    assert_eq!(config.rate_limit.per_ip_requests_per_minute, 30);
    assert_eq!(config.rate_limit.ip_memory_duration, 3600);
    assert_eq!(config.rate_limit.refill_interval, 1);
}

#[test]
fn test_default_server_config() {
    let server_config = ServerConfig::default();

    assert_eq!(
        server_config.default_host,
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
    );
    assert_eq!(server_config.default_port, 8080);
    assert_eq!(server_config.healthcheck_token, None);
    assert_eq!(server_config.healthcheck_host_bypass, None);
}

#[test]
fn test_default_github_config() {
    let github_config = GitHubConfig::default();

    assert_eq!(github_config.token, None);
    assert_eq!(github_config.retry_attempts, 3);
}

#[test]
fn test_default_rate_limit_config() {
    let rate_limit_config = RateLimitConfig::default();

    assert_eq!(rate_limit_config.global_requests_per_minute, 300);
    assert_eq!(rate_limit_config.per_ip_requests_per_minute, 30);
    assert_eq!(rate_limit_config.ip_memory_duration, 3600);
    assert_eq!(rate_limit_config.refill_interval, 1);
}

#[test]
fn test_config_load_without_overrides() {
    // Clear environment variables to test defaults
    std::env::remove_var("GITHUB_TOKEN");
    std::env::remove_var("PORT");
    std::env::remove_var("HEALTHCHECK_TOKEN");
    std::env::remove_var("HEALTHCHECK_HOST_BYPASS");

    let config = Config::load(None);

    // Should use defaults when no ENV vars are set
    assert_eq!(config.github.token, None);
    assert_eq!(config.server.default_port, 8080);
    assert_eq!(config.server.healthcheck_token, None);
    assert_eq!(config.server.healthcheck_host_bypass, None);
}

#[test]
fn test_config_load_with_cli_overrides() {
    // Clear environment variables
    std::env::remove_var("GITHUB_TOKEN");
    std::env::remove_var("PORT");

    let cli_overrides = CliOverrides::from_cli_args(Some("cli-token".to_string()), Some(9000));

    let config = Config::load(Some(cli_overrides));

    // CLI overrides should take precedence
    assert_eq!(config.github.token, Some("cli-token".to_string()));
    assert_eq!(config.server.default_port, 9000);
}

#[test]
fn test_config_load_with_environment_variables() {
    // Set environment variables
    std::env::set_var("GITHUB_TOKEN", "env-token");
    std::env::set_var("PORT", "5000");
    std::env::set_var("HEALTHCHECK_TOKEN", "health-token");
    std::env::set_var("HEALTHCHECK_HOST_BYPASS", "localhost");

    let config = Config::load(None);

    // Should load from environment variables
    assert_eq!(config.github.token, Some("env-token".to_string()));
    assert_eq!(config.server.default_port, 5000);
    assert_eq!(
        config.server.healthcheck_token,
        Some("health-token".to_string())
    );
    assert_eq!(
        config.server.healthcheck_host_bypass,
        Some("localhost".to_string())
    );

    // Clean up
    std::env::remove_var("GITHUB_TOKEN");
    std::env::remove_var("PORT");
    std::env::remove_var("HEALTHCHECK_TOKEN");
    std::env::remove_var("HEALTHCHECK_HOST_BYPASS");
}

#[test]
fn test_cli_overrides_take_precedence() {
    // Set environment variables
    std::env::set_var("GITHUB_TOKEN", "env-token");
    std::env::set_var("PORT", "5000");

    let cli_overrides = CliOverrides::from_cli_args(Some("cli-token".to_string()), Some(9000));

    let config = Config::load(Some(cli_overrides));

    // CLI overrides should take precedence over ENV vars
    assert_eq!(config.github.token, Some("cli-token".to_string()));
    assert_eq!(config.server.default_port, 9000);

    // Clean up
    std::env::remove_var("GITHUB_TOKEN");
    std::env::remove_var("PORT");
}

#[test]
fn test_invalid_port_environment_variable() {
    // Set invalid PORT value
    std::env::set_var("PORT", "invalid-port");
    std::env::remove_var("GITHUB_TOKEN");

    let config = Config::load(None);

    // Should fall back to default port when PORT is invalid
    assert_eq!(config.server.default_port, 8080);

    // Clean up
    std::env::remove_var("PORT");
}

#[test]
fn test_port_environment_variable_edge_cases() {
    // Test various invalid PORT values
    let invalid_ports = ["", "65536", "abc", "123abc", "-1"];

    for invalid_port in invalid_ports {
        std::env::set_var("PORT", invalid_port);
        std::env::remove_var("GITHUB_TOKEN");

        let config = Config::load(None);
        assert_eq!(
            config.server.default_port, 8080,
            "Failed for PORT={}",
            invalid_port
        );

        std::env::remove_var("PORT");
    }
}

#[test]
fn test_valid_port_environment_variable() {
    // Test valid PORT values
    let valid_ports = ["1", "8080", "9000", "65535"];

    for valid_port in valid_ports {
        std::env::set_var("PORT", valid_port);
        std::env::remove_var("GITHUB_TOKEN");

        let config = Config::load(None);
        let expected_port = valid_port.parse::<u16>().unwrap();
        assert_eq!(
            config.server.default_port, expected_port,
            "Failed for PORT={}",
            valid_port
        );

        std::env::remove_var("PORT");
    }
}

#[test]
fn test_config_getter_methods() {
    let mut config = Config::default();

    // Set some values to test getters
    config.github.token = Some("test-token".to_string());
    config.server.default_port = 9000;
    config.server.healthcheck_token = Some("health-token".to_string());
    config.server.healthcheck_host_bypass = Some("localhost".to_string());

    // Test getter methods
    assert_eq!(
        config.default_host(),
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
    );
    assert_eq!(config.default_port(), 9000);
    assert_eq!(config.github_token(), Some("test-token"));
    assert_eq!(config.healthcheck_token(), Some("health-token"));
    assert_eq!(config.healthcheck_host_bypass(), Some("localhost"));
    assert_eq!(config.rate_limit_config().global_requests_per_minute, 300);
}

#[test]
fn test_config_getter_methods_with_none_values() {
    let config = Config::default();

    // Test getters with None values
    assert_eq!(config.github_token(), None);
    assert_eq!(config.healthcheck_token(), None);
    assert_eq!(config.healthcheck_host_bypass(), None);
}

#[test]
fn test_cli_overrides_from_cli_args() {
    let overrides = CliOverrides::from_cli_args(Some("test-token".to_string()), Some(9000));

    assert_eq!(overrides.token, Some("test-token".to_string()));
    assert_eq!(overrides.port, Some(9000));
}

#[test]
fn test_cli_overrides_with_none_values() {
    let overrides = CliOverrides::from_cli_args(None, None);

    assert_eq!(overrides.token, None);
    assert_eq!(overrides.port, None);
}

#[test]
fn test_config_clone() {
    let config1 = Config::default();
    let config2 = config1.clone();

    // Test that clone creates identical copy
    assert_eq!(config1.server.default_port, config2.server.default_port);
    assert_eq!(config1.github.token, config2.github.token);
    assert_eq!(
        config1.rate_limit.global_requests_per_minute,
        config2.rate_limit.global_requests_per_minute
    );
}

#[test]
fn test_config_debug_format() {
    let config = Config::default();
    let debug_str = format!("{:?}", config);

    // Debug format should contain key information
    assert!(debug_str.contains("Config"));
    assert!(debug_str.contains("ServerConfig"));
    assert!(debug_str.contains("GitHubConfig"));
    assert!(debug_str.contains("RateLimitConfig"));
}

#[test]
fn test_partial_cli_overrides() {
    // Test that only some CLI overrides are applied
    let cli_overrides = CliOverrides::from_cli_args(
        Some("cli-token".to_string()),
        None, // No port override
    );

    std::env::set_var("PORT", "5000");
    std::env::remove_var("GITHUB_TOKEN");

    let config = Config::load(Some(cli_overrides));

    // Token should come from CLI, port from ENV
    assert_eq!(config.github.token, Some("cli-token".to_string()));
    assert_eq!(config.server.default_port, 5000);

    // Clean up
    std::env::remove_var("PORT");
}

#[test]
fn test_empty_environment_variables() {
    // Test that empty environment variables are handled correctly
    std::env::set_var("GITHUB_TOKEN", "");
    std::env::set_var("PORT", "");
    std::env::set_var("HEALTHCHECK_TOKEN", "");
    std::env::set_var("HEALTHCHECK_HOST_BYPASS", "");

    let config = Config::load(None);

    // Empty strings are preserved as Some("") rather than None
    assert_eq!(config.github.token, Some("".to_string()));
    assert_eq!(config.server.default_port, 8080); // Should fall back to default
    assert_eq!(config.server.healthcheck_token, Some("".to_string())); // Empty string is preserved
    assert_eq!(config.server.healthcheck_host_bypass, Some("".to_string())); // Empty string is preserved

    // Clean up
    std::env::remove_var("GITHUB_TOKEN");
    std::env::remove_var("PORT");
    std::env::remove_var("HEALTHCHECK_TOKEN");
    std::env::remove_var("HEALTHCHECK_HOST_BYPASS");
}

#[test]
fn test_config_load_with_mixed_scenarios() {
    // Test a complex scenario with mixed CLI and ENV values
    std::env::set_var("GITHUB_TOKEN", "env-token");
    std::env::set_var("PORT", "5000");
    std::env::set_var("HEALTHCHECK_TOKEN", "health-token");
    std::env::set_var("HEALTHCHECK_HOST_BYPASS", "localhost");

    let cli_overrides = CliOverrides::from_cli_args(
        Some("cli-token".to_string()),
        None, // No CLI port override
    );

    let config = Config::load(Some(cli_overrides));

    // CLI token should override ENV token
    assert_eq!(config.github.token, Some("cli-token".to_string()));
    // Port should come from ENV since no CLI override
    assert_eq!(config.server.default_port, 5000);
    // Health check values should come from ENV
    assert_eq!(
        config.server.healthcheck_token,
        Some("health-token".to_string())
    );
    assert_eq!(
        config.server.healthcheck_host_bypass,
        Some("localhost".to_string())
    );

    // Clean up
    std::env::remove_var("GITHUB_TOKEN");
    std::env::remove_var("PORT");
    std::env::remove_var("HEALTHCHECK_TOKEN");
    std::env::remove_var("HEALTHCHECK_HOST_BYPASS");
}
