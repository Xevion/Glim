//! Rate limiting system for preventing abuse.
//!
//! Provides both global and per-IP rate limiting using a token bucket algorithm
//! with automatic cleanup of old entries.

use std::net::IpAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;
use moka::future::Cache;
use tracing::{debug, warn};

/// Configuration for rate limiting
#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    /// Maximum requests per minute globally
    pub global_requests_per_minute: u32,
    /// Maximum requests per minute per IP
    pub per_ip_requests_per_minute: u32,
    /// How long to remember IPs (in seconds)
    pub ip_memory_duration: u64,
    /// How often to refill tokens (in seconds)
    pub refill_interval: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            global_requests_per_minute: 300,      // 300 requests per minute globally
            per_ip_requests_per_minute: 30,       // 30 requests per minute per IP
            ip_memory_duration: 3600,             // 1 hour
            refill_interval: 1,                   // Refill every second
        }
    }
}

/// Token bucket for rate limiting
#[derive(Debug)]
struct TokenBucket {
    tokens: AtomicU32,
    max_tokens: u32,
    refill_rate: u32, // tokens per refill interval
    last_refill: RwLock<Instant>,
}

impl TokenBucket {
    fn new(max_tokens: u32, refill_rate: u32) -> Self {
        Self {
            tokens: AtomicU32::new(max_tokens),
            max_tokens,
            refill_rate,
            last_refill: RwLock::new(Instant::now()),
        }
    }

    /// Try to consume a token. Returns true if successful, false if rate limited.
    async fn try_consume(&self) -> bool {
        self.refill().await;
        
        // Use a loop instead of recursion to avoid boxing
        loop {
            let current_tokens = self.tokens.load(Ordering::Acquire);
            if current_tokens > 0 {
                // Try to decrement atomically
                match self.tokens.compare_exchange_weak(
                    current_tokens,
                    current_tokens - 1,
                    Ordering::Release,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return true,
                    Err(_) => {
                        // Someone else consumed the token, try again
                        continue;
                    }
                }
            } else {
                return false;
            }
        }
    }

    /// Refill tokens based on elapsed time
    async fn refill(&self) {
        let now = Instant::now();
        let mut last_refill = self.last_refill.write().await;
        
        let elapsed = now.duration_since(*last_refill);
        if elapsed >= Duration::from_secs(1) {
            let seconds_passed = elapsed.as_secs() as u32;
            let tokens_to_add = seconds_passed * self.refill_rate;
            
            if tokens_to_add > 0 {
                let current_tokens = self.tokens.load(Ordering::Acquire);
                let new_tokens = (current_tokens + tokens_to_add).min(self.max_tokens);
                self.tokens.store(new_tokens, Ordering::Release);
                *last_refill = now;
                
                // Only log if we actually added tokens and it's significant
                if tokens_to_add > 0 && current_tokens < self.max_tokens / 2 {
                    debug!("Refilled {} tokens, current: {}/{}", tokens_to_add, new_tokens, self.max_tokens);
                }
            }
        }
    }

    /// Get current token count (for monitoring)
    fn current_tokens(&self) -> u32 {
        self.tokens.load(Ordering::Acquire)
    }
}

/// Rate limiter with global and per-IP limits
#[derive(Clone, Debug)]
pub struct RateLimiter {
    config: RateLimitConfig,
    global_bucket: Arc<TokenBucket>,
    ip_buckets: Cache<IpAddr, Arc<TokenBucket>>,
}

impl RateLimiter {
    /// Create a new rate limiter with the given configuration
    pub fn new(config: RateLimitConfig) -> Self {
        let global_bucket = Arc::new(TokenBucket::new(
            config.global_requests_per_minute,
            std::cmp::max(1, config.global_requests_per_minute / 60), // at least 1 per second
        ));

        // Cache for per-IP buckets with TTL
        let ip_buckets = Cache::builder()
            .max_capacity(10_000) // Limit memory usage
            .time_to_live(Duration::from_secs(config.ip_memory_duration))
            .build();

        let limiter = Self {
            config,
            global_bucket,
            ip_buckets,
        };

        // Start background task for periodic refilling
        limiter.start_refill_task();

        limiter
    }

    /// Check if a request from the given IP should be allowed
    pub async fn check_rate_limit(&self, ip: IpAddr) -> RateLimitResult {
        // First check global rate limit
        if !self.global_bucket.try_consume().await {
            warn!("Global rate limit exceeded");
            return RateLimitResult::GlobalLimitExceeded;
        }

        // Then check per-IP rate limit
        let ip_bucket = self.get_or_create_ip_bucket(ip).await;
        if !ip_bucket.try_consume().await {
            warn!("Rate limit exceeded for IP: {}", ip);
            return RateLimitResult::IpLimitExceeded;
        }

        RateLimitResult::Allowed
    }

    /// Get or create a token bucket for the given IP
    async fn get_or_create_ip_bucket(&self, ip: IpAddr) -> Arc<TokenBucket> {
        if let Some(bucket) = self.ip_buckets.get(&ip).await {
            bucket
        } else {
            let bucket = Arc::new(TokenBucket::new(
                self.config.per_ip_requests_per_minute,
                std::cmp::max(1, self.config.per_ip_requests_per_minute / 60), // at least 1 per second
            ));
            self.ip_buckets.insert(ip, bucket.clone()).await;
            bucket
        }
    }

    /// Start background task for periodic token refilling
    fn start_refill_task(&self) {
        let global_bucket = Arc::clone(&self.global_bucket);
        let ip_buckets = self.ip_buckets.clone();
        let refill_interval = self.config.refill_interval;

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(refill_interval));
            
            loop {
                interval.tick().await;
                
                // Refill global bucket
                global_bucket.refill().await;
                
                // Refill all IP buckets
                // Note: moka automatically handles cleanup of expired entries
                for (_ip, bucket) in ip_buckets.iter() {
                    bucket.refill().await;
                }
            }
        });
    }

    /// Get current status for monitoring
    pub async fn status(&self) -> RateLimitStatus {
        let global_tokens = self.global_bucket.current_tokens();
        let active_ips = self.ip_buckets.weighted_size() as u32;
        
        RateLimitStatus {
            global_tokens_remaining: global_tokens,
            global_tokens_max: self.config.global_requests_per_minute,
            active_ip_count: active_ips,
            config: self.config.clone(),
        }
    }
}

/// Result of a rate limit check
#[derive(Debug, Clone, PartialEq)]
pub enum RateLimitResult {
    /// Request is allowed
    Allowed,
    /// Global rate limit exceeded
    GlobalLimitExceeded,
    /// Per-IP rate limit exceeded
    IpLimitExceeded,
}

/// Status information for monitoring
#[derive(Debug, Clone)]
pub struct RateLimitStatus {
    pub global_tokens_remaining: u32,
    pub global_tokens_max: u32,
    pub active_ip_count: u32,
    pub config: RateLimitConfig,
}

impl std::fmt::Display for RateLimitStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{\"global_tokens_remaining\": {}, \"global_tokens_max\": {}, \"active_ip_count\": {}, \"global_rpm\": {}, \"per_ip_rpm\": {}}}",
            self.global_tokens_remaining,
            self.global_tokens_max,
            self.active_ip_count,
            self.config.global_requests_per_minute,
            self.config.per_ip_requests_per_minute
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_token_bucket_basic() {
        let bucket = TokenBucket::new(5, 1);
        
        // Should be able to consume up to max tokens
        for _ in 0..5 {
            assert!(bucket.try_consume().await);
        }
        
        // Should be rate limited after consuming all tokens
        assert!(!bucket.try_consume().await);
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let bucket = TokenBucket::new(2, 1);
        
        // Consume all tokens
        assert!(bucket.try_consume().await);
        assert!(bucket.try_consume().await);
        assert!(!bucket.try_consume().await);
        
        // Wait for refill
        sleep(Duration::from_secs(2)).await;
        
        // Should have tokens again
        assert!(bucket.try_consume().await);
    }

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
        assert_eq!(limiter.check_rate_limit(ip).await, RateLimitResult::GlobalLimitExceeded);
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
        assert_eq!(limiter.check_rate_limit(ip).await, RateLimitResult::IpLimitExceeded);
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
        assert_eq!(limiter.check_rate_limit(ip1).await, RateLimitResult::Allowed);
        assert_eq!(limiter.check_rate_limit(ip2).await, RateLimitResult::Allowed);
        
        // Both should be rate limited after consuming their tokens
        assert_eq!(limiter.check_rate_limit(ip1).await, RateLimitResult::IpLimitExceeded);
        assert_eq!(limiter.check_rate_limit(ip2).await, RateLimitResult::IpLimitExceeded);
    }
}
