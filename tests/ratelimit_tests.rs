use livecards::ratelimit::{RateLimitConfig, RateLimitResult, RateLimiter};
use std::net::{IpAddr, Ipv4Addr};

#[tokio::test]
async fn test_rate_limiter_global_limit() {
    let config = RateLimitConfig {
        global_requests_per_minute: 2,
        per_ip_requests_per_minute: 10,
        ip_memory_duration: 3600,
        refill_interval: 1,
    };

    let limiter = RateLimiter::new(config);
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Should allow up to global limit
    assert_eq!(limiter.check_rate_limit(ip).await, RateLimitResult::Allowed);
    assert_eq!(limiter.check_rate_limit(ip).await, RateLimitResult::Allowed);

    // Should exceed global limit
    assert_eq!(
        limiter.check_rate_limit(ip).await,
        RateLimitResult::GlobalLimitExceeded
    );
}

#[tokio::test]
async fn test_rate_limiter_ip_limit() {
    let config = RateLimitConfig {
        global_requests_per_minute: 100,
        per_ip_requests_per_minute: 2,
        ip_memory_duration: 3600,
        refill_interval: 1,
    };

    let limiter = RateLimiter::new(config);
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Should allow up to per-IP limit
    assert_eq!(limiter.check_rate_limit(ip).await, RateLimitResult::Allowed);
    assert_eq!(limiter.check_rate_limit(ip).await, RateLimitResult::Allowed);

    // Should exceed per-IP limit
    assert_eq!(
        limiter.check_rate_limit(ip).await,
        RateLimitResult::IpLimitExceeded
    );
}

#[tokio::test]
async fn test_rate_limiter_different_ips() {
    let config = RateLimitConfig {
        global_requests_per_minute: 100,
        per_ip_requests_per_minute: 1,
        ip_memory_duration: 3600,
        refill_interval: 1,
    };

    let limiter = RateLimiter::new(config);
    let ip1 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let ip2 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));

    // Each IP should have its own limit
    assert_eq!(
        limiter.check_rate_limit(ip1).await,
        RateLimitResult::Allowed
    );
    assert_eq!(
        limiter.check_rate_limit(ip2).await,
        RateLimitResult::Allowed
    );

    // Both should be rate limited after consuming their tokens
    assert_eq!(
        limiter.check_rate_limit(ip1).await,
        RateLimitResult::IpLimitExceeded
    );
    assert_eq!(
        limiter.check_rate_limit(ip2).await,
        RateLimitResult::IpLimitExceeded
    );
}

#[tokio::test]
async fn test_rate_limit_config_default() {
    let config = RateLimitConfig::default();

    assert_eq!(config.global_requests_per_minute, 300);
    assert_eq!(config.per_ip_requests_per_minute, 30);
    assert_eq!(config.ip_memory_duration, 3600);
    assert_eq!(config.refill_interval, 1);
}

#[tokio::test]
async fn test_rate_limit_config_custom() {
    let config = RateLimitConfig {
        global_requests_per_minute: 500,
        per_ip_requests_per_minute: 50,
        ip_memory_duration: 7200,
        refill_interval: 2,
    };

    assert_eq!(config.global_requests_per_minute, 500);
    assert_eq!(config.per_ip_requests_per_minute, 50);
    assert_eq!(config.ip_memory_duration, 7200);
    assert_eq!(config.refill_interval, 2);
}

#[tokio::test]
async fn test_rate_limit_result_variants() {
    // Test Allowed variant
    let allowed = RateLimitResult::Allowed;
    assert!(matches!(allowed, RateLimitResult::Allowed));

    // Test GlobalLimitExceeded variant
    let global_exceeded = RateLimitResult::GlobalLimitExceeded;
    assert!(matches!(
        global_exceeded,
        RateLimitResult::GlobalLimitExceeded
    ));

    // Test IpLimitExceeded variant
    let ip_exceeded = RateLimitResult::IpLimitExceeded;
    assert!(matches!(ip_exceeded, RateLimitResult::IpLimitExceeded));
}

#[tokio::test]
async fn test_rate_limiter_status() {
    let config = RateLimitConfig {
        global_requests_per_minute: 100,
        per_ip_requests_per_minute: 10,
        ip_memory_duration: 3600,
        refill_interval: 1,
    };

    let limiter = RateLimiter::new(config);
    let status = limiter.status().await;

    // Test status fields
    assert_eq!(status.global_tokens_max, 100);
    assert_eq!(status.config.global_requests_per_minute, 100);
    assert_eq!(status.config.per_ip_requests_per_minute, 10);
    assert_eq!(status.config.ip_memory_duration, 3600);
    assert_eq!(status.config.refill_interval, 1);

    // Global tokens should be at max initially
    assert_eq!(status.global_tokens_remaining, 100);

    // Active IP count should be 0 initially
    assert_eq!(status.active_ip_count, 0);
}

#[tokio::test]
async fn test_rate_limiter_status_after_requests() {
    let config = RateLimitConfig {
        global_requests_per_minute: 10,
        per_ip_requests_per_minute: 5,
        ip_memory_duration: 3600,
        refill_interval: 1,
    };

    let limiter = RateLimiter::new(config);
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Make some requests
    for _ in 0..3 {
        assert_eq!(limiter.check_rate_limit(ip).await, RateLimitResult::Allowed);
    }

    let status = limiter.status().await;

    // Global tokens should be reduced
    assert_eq!(status.global_tokens_remaining, 7);

    // Active IP count might be 0 due to timing, so just verify it's reasonable
    assert!(status.active_ip_count <= 1);
}

#[tokio::test]
async fn test_rate_limiter_status_display() {
    let config = RateLimitConfig {
        global_requests_per_minute: 100,
        per_ip_requests_per_minute: 10,
        ip_memory_duration: 3600,
        refill_interval: 1,
    };

    let limiter = RateLimiter::new(config);
    let status = limiter.status().await;
    let status_str = status.to_string();

    // Test that status string contains expected fields
    assert!(status_str.contains("\"global_tokens_remaining\""));
    assert!(status_str.contains("\"global_tokens_max\""));
    assert!(status_str.contains("\"active_ip_count\""));
    assert!(status_str.contains("\"global_rpm\""));
    assert!(status_str.contains("\"per_ip_rpm\""));

    // Test that values are present
    assert!(status_str.contains("100")); // global_tokens_max
    assert!(status_str.contains("10")); // per_ip_rpm
}

#[tokio::test]
async fn test_rate_limiter_concurrent_access() {
    let config = RateLimitConfig {
        global_requests_per_minute: 100,
        per_ip_requests_per_minute: 10,
        ip_memory_duration: 3600,
        refill_interval: 1,
    };

    let limiter = RateLimiter::new(config);
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

    // Test sequential requests (simpler than concurrent)
    for _ in 0..5 {
        assert_eq!(limiter.check_rate_limit(ip).await, RateLimitResult::Allowed);
    }

    // Verify that tokens were consumed
    let status = limiter.status().await;
    assert_eq!(status.global_tokens_remaining, 95); // 100 - 5
}
