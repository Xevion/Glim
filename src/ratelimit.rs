//! Rate limiting system for preventing abuse.
//!
//! Provides both global and per-IP rate limiting using a token bucket algorithm
//! with automatic cleanup of old entries.

use moka::future::Cache;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, warn};

/// Time provider trait for mocking in tests
#[cfg(test)]
pub trait TimeProvider {
    fn now(&self) -> Instant;
    fn advance(&mut self, duration: Duration);
}

/// Real time provider for production
#[cfg(test)]
pub struct RealTimeProvider;

#[cfg(test)]
impl TimeProvider for RealTimeProvider {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn advance(&mut self, _duration: Duration) {
        // No-op for real time
    }
}

/// Mock time provider for tests
#[cfg(test)]
pub struct MockTimeProvider {
    current_time: Instant,
}

#[cfg(test)]
impl MockTimeProvider {
    pub fn new() -> Self {
        Self {
            current_time: Instant::now(),
        }
    }
}

#[cfg(test)]
impl Default for MockTimeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl TimeProvider for MockTimeProvider {
    fn now(&self) -> Instant {
        self.current_time
    }

    fn advance(&mut self, duration: Duration) {
        self.current_time += duration;
    }
}

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
            global_requests_per_minute: 300, // 300 requests per minute globally
            per_ip_requests_per_minute: 30,  // 30 requests per minute per IP
            ip_memory_duration: 3600,        // 1 hour
            refill_interval: 1,              // Refill every second
        }
    }
}

/// Token bucket for rate limiting
#[cfg(test)]
pub struct TokenBucket {
    tokens: AtomicU32,
    max_tokens: u32,
    refill_rate: u32, // tokens per refill interval
    last_refill: RwLock<Instant>,
    time_provider: Arc<RwLock<Box<dyn TimeProvider + Send + Sync>>>,
}

#[cfg(not(test))]
struct TokenBucket {
    tokens: AtomicU32,
    max_tokens: u32,
    refill_rate: u32, // tokens per refill interval
    last_refill: RwLock<Instant>,
}

impl std::fmt::Debug for TokenBucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenBucket")
            .field("tokens", &self.tokens.load(Ordering::Acquire))
            .field("max_tokens", &self.max_tokens)
            .field("refill_rate", &self.refill_rate)
            .field(
                "last_refill",
                &self
                    .last_refill
                    .try_read()
                    .map(|r| *r)
                    .unwrap_or_else(|_| Instant::now()),
            )
            .finish()
    }
}

impl TokenBucket {
    #[cfg(not(test))]
    fn new(max_tokens: u32, refill_rate: u32) -> Self {
        Self {
            tokens: AtomicU32::new(max_tokens),
            max_tokens,
            refill_rate,
            last_refill: RwLock::new(Instant::now()),
        }
    }

    #[cfg(test)]
    pub fn new(max_tokens: u32, refill_rate: u32) -> Self {
        Self {
            tokens: AtomicU32::new(max_tokens),
            max_tokens,
            refill_rate,
            last_refill: RwLock::new(Instant::now()),
            time_provider: Arc::new(RwLock::new(Box::new(RealTimeProvider))),
        }
    }

    #[cfg(test)]
    pub fn new_with_time_provider(
        max_tokens: u32,
        refill_rate: u32,
        time_provider: Box<dyn TimeProvider + Send + Sync>,
    ) -> Self {
        Self {
            tokens: AtomicU32::new(max_tokens),
            max_tokens,
            refill_rate,
            last_refill: RwLock::new(Instant::now()),
            time_provider: Arc::new(RwLock::new(time_provider)),
        }
    }

    /// Try to consume a token. Returns true if successful, false if rate limited.
    #[cfg(not(test))]
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

    #[cfg(test)]
    pub async fn try_consume(&self) -> bool {
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
    #[cfg(not(test))]
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
                    debug!(
                        "Refilled {} tokens, current: {}/{}",
                        tokens_to_add, new_tokens, self.max_tokens
                    );
                }
            }
        }
    }

    #[cfg(test)]
    pub async fn refill(&self) {
        #[cfg(test)]
        let now = {
            let time_provider = self.time_provider.read().await;
            time_provider.now()
        };
        #[cfg(not(test))]
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
                    debug!(
                        "Refilled {} tokens, current: {}/{}",
                        tokens_to_add, new_tokens, self.max_tokens
                    );
                }
            }
        }
    }

    /// Get current token count (for monitoring)
    fn current_tokens(&self) -> u32 {
        self.tokens.load(Ordering::Acquire)
    }

    #[cfg(test)]
    /// Advance time for testing
    pub async fn advance_time(&self, duration: Duration) {
        let mut time_provider = self.time_provider.write().await;
        time_provider.advance(duration);
    }
}

/// Rate limiter with global and per-IP limits
#[derive(Clone)]
pub struct RateLimiter {
    config: RateLimitConfig,
    global_bucket: Arc<TokenBucket>,
    ip_buckets: Cache<IpAddr, Arc<TokenBucket>>,
}

impl std::fmt::Debug for RateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimiter")
            .field("config", &self.config)
            .field("global_bucket", &self.global_bucket)
            .field("ip_buckets_count", &self.ip_buckets.weighted_size())
            .finish()
    }
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
