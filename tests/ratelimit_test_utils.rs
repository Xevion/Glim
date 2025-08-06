use std::time::{Duration, Instant};

/// Time provider trait for mocking in tests
pub trait TimeProvider {
    fn now(&self) -> Instant;
    fn advance(&mut self, duration: Duration);
}

/// Real time provider for production
pub struct RealTimeProvider;

impl TimeProvider for RealTimeProvider {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn advance(&mut self, _duration: Duration) {
        // No-op for real time
    }
}

/// Mock time provider for tests
pub struct MockTimeProvider {
    current_time: Instant,
}

impl MockTimeProvider {
    pub fn new() -> Self {
        Self {
            current_time: Instant::now(),
        }
    }
}

impl Default for MockTimeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeProvider for MockTimeProvider {
    fn now(&self) -> Instant {
        self.current_time
    }

    fn advance(&mut self, duration: Duration) {
        self.current_time += duration;
    }
}
