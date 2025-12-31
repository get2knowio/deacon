//! Retry and backoff utilities for network and Docker operations
//!
//! This module provides configurable retry mechanisms with exponential backoff
//! and jitter to improve resilience of network operations and container runtime
//! interactions.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, instrument, warn};

/// Jitter strategy for retry delays
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum JitterStrategy {
    /// Full jitter: random delay between 0 and calculated delay
    #[default]
    FullJitter,
    /// Equal jitter: half calculated delay plus random half  
    EqualJitter,
}

/// Configuration for retry behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (excluding initial attempt)
    pub max_attempts: u32,
    /// Base delay for exponential backoff
    pub base_delay: Duration,
    /// Maximum delay cap
    pub max_delay: Duration,
    /// Jitter strategy to apply
    pub jitter: JitterStrategy,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            jitter: JitterStrategy::default(),
        }
    }
}

impl RetryConfig {
    /// Create a new RetryConfig with specified parameters
    pub fn new(
        max_attempts: u32,
        base_delay: Duration,
        max_delay: Duration,
        jitter: JitterStrategy,
    ) -> Self {
        Self {
            max_attempts,
            base_delay,
            max_delay,
            jitter,
        }
    }

    /// Calculate delay for a given attempt number (0-based)
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        // Exponential backoff: base_delay * 2^attempt
        let exponential_delay = self
            .base_delay
            .as_millis()
            .saturating_mul(2_u128.pow(attempt));

        // Cap at max_delay
        let capped_delay = exponential_delay.min(self.max_delay.as_millis());
        let delay_ms = capped_delay as u64;

        self.apply_jitter(Duration::from_millis(delay_ms))
    }

    /// Apply jitter to the calculated delay
    fn apply_jitter(&self, delay: Duration) -> Duration {
        match self.jitter {
            JitterStrategy::FullJitter => {
                // Random delay between 0 and full calculated delay
                let jitter_ms = fastrand::u64(0..=delay.as_millis() as u64);
                Duration::from_millis(jitter_ms)
            }
            JitterStrategy::EqualJitter => {
                // Half calculated delay plus random half
                let half_delay = delay.as_millis() as u64 / 2;
                let jitter_ms = half_delay + fastrand::u64(0..=half_delay);
                Duration::from_millis(jitter_ms)
            }
        }
    }

    /// Apply jitter with seeded RNG for deterministic testing
    #[cfg(test)]
    fn apply_jitter_seeded(&self, delay: Duration, seed: u64) -> Duration {
        let mut rng = fastrand::Rng::with_seed(seed);
        match self.jitter {
            JitterStrategy::FullJitter => {
                let jitter_ms = rng.u64(0..=delay.as_millis() as u64);
                Duration::from_millis(jitter_ms)
            }
            JitterStrategy::EqualJitter => {
                let half_delay = delay.as_millis() as u64 / 2;
                let jitter_ms = half_delay + rng.u64(0..=half_delay);
                Duration::from_millis(jitter_ms)
            }
        }
    }

    /// Calculate delay with seeded RNG for testing
    #[cfg(test)]
    pub fn calculate_delay_seeded(&self, attempt: u32, seed: u64) -> Duration {
        let exponential_delay = self
            .base_delay
            .as_millis()
            .saturating_mul(2_u128.pow(attempt));
        let capped_delay = exponential_delay.min(self.max_delay.as_millis());
        let delay_ms = capped_delay as u64;

        self.apply_jitter_seeded(Duration::from_millis(delay_ms), seed)
    }
}

/// Error classification result for retry decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Retry the operation
    Retry,
    /// Do not retry (terminal error)
    Stop,
}

/// Error classifier function type
pub type ErrorClassifier<E> = fn(&E) -> RetryDecision;

/// Default error classifier that retries on all errors
pub fn default_classifier<E>(_error: &E) -> RetryDecision {
    RetryDecision::Retry
}

/// Retry an async operation with exponential backoff and jitter
#[instrument(level = "debug", skip(operation, classify_error))]
pub async fn retry_async<T, E, Fut, Op>(
    config: &RetryConfig,
    operation: Op,
    classify_error: ErrorClassifier<E>,
) -> std::result::Result<T, E>
where
    Op: Fn() -> Fut,
    Fut: std::future::Future<Output = std::result::Result<T, E>>,
    E: std::fmt::Debug,
{
    let mut last_error = None;

    for attempt in 0..=config.max_attempts {
        debug!("Retry attempt {} of {}", attempt, config.max_attempts);

        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!("Operation succeeded on attempt {}", attempt);
                }
                return Ok(result);
            }
            Err(error) => {
                debug!("Operation failed on attempt {}: {:?}", attempt, error);

                // Check if we should retry this error
                if classify_error(&error) == RetryDecision::Stop {
                    debug!("Error classifier indicated stop, not retrying");
                    return Err(error);
                }

                last_error = Some(error);

                // Don't sleep after the last attempt
                if attempt < config.max_attempts {
                    let delay = config.calculate_delay(attempt);
                    debug!("Sleeping for {:?} before next attempt", delay);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    // All attempts exhausted
    let final_error = last_error.expect("Should have at least one error");
    warn!(
        "All {} retry attempts exhausted, final error: {:?}",
        config.max_attempts + 1,
        final_error
    );
    Err(final_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.base_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(30));
        assert_eq!(config.jitter, JitterStrategy::FullJitter);
    }

    #[test]
    fn test_retry_config_new() {
        let config = RetryConfig::new(
            5,
            Duration::from_millis(200),
            Duration::from_secs(60),
            JitterStrategy::EqualJitter,
        );
        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.base_delay, Duration::from_millis(200));
        assert_eq!(config.max_delay, Duration::from_secs(60));
        assert_eq!(config.jitter, JitterStrategy::EqualJitter);
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            jitter: JitterStrategy::FullJitter,
        };

        // Test delay calculation with seeded RNG for deterministic results
        let seed = 12345;

        // Attempt 0: base_delay * 2^0 = 100ms
        let delay0 = config.calculate_delay_seeded(0, seed);
        assert!(delay0.as_millis() <= 100); // With FullJitter, should be <= base

        // Attempt 1: base_delay * 2^1 = 200ms
        let delay1 = config.calculate_delay_seeded(1, seed);
        assert!(delay1.as_millis() <= 200);

        // Attempt 2: base_delay * 2^2 = 400ms
        let delay2 = config.calculate_delay_seeded(2, seed);
        assert!(delay2.as_millis() <= 400);
    }

    #[test]
    fn test_max_delay_cap() {
        let config = RetryConfig {
            max_attempts: 10,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(500), // Small cap
            jitter: JitterStrategy::FullJitter,
        };

        let seed = 12345;

        // High attempt number should be capped at max_delay
        let delay = config.calculate_delay_seeded(10, seed);
        assert!(delay.as_millis() <= 500);
    }

    #[test]
    fn test_jitter_strategies() {
        let base_delay = Duration::from_millis(1000);
        let config_full = RetryConfig {
            max_attempts: 1,
            base_delay,
            max_delay: Duration::from_secs(10),
            jitter: JitterStrategy::FullJitter,
        };
        let config_equal = RetryConfig {
            max_attempts: 1,
            base_delay,
            max_delay: Duration::from_secs(10),
            jitter: JitterStrategy::EqualJitter,
        };

        let seed = 12345;

        // FullJitter: should be between 0 and full delay
        let full_jitter_delay = config_full.apply_jitter_seeded(base_delay, seed);
        assert!(full_jitter_delay.as_millis() <= base_delay.as_millis());

        // EqualJitter: should be between half delay and full delay
        let equal_jitter_delay = config_equal.apply_jitter_seeded(base_delay, seed);
        let half_delay = base_delay.as_millis() / 2;
        assert!(equal_jitter_delay.as_millis() >= half_delay);
        assert!(equal_jitter_delay.as_millis() <= base_delay.as_millis());
    }

    #[test]
    fn test_error_classifier() {
        // Test default classifier
        let decision = default_classifier(&"any error");
        assert_eq!(decision, RetryDecision::Retry);

        // Test custom classifier
        let custom_classifier = |error: &i32| {
            if *error == 404 {
                RetryDecision::Stop
            } else {
                RetryDecision::Retry
            }
        };

        assert_eq!(custom_classifier(&500), RetryDecision::Retry);
        assert_eq!(custom_classifier(&404), RetryDecision::Stop);
    }

    #[tokio::test]
    async fn test_retry_async_success_on_first_attempt() {
        let config = RetryConfig::default();
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let operation = move || {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok::<i32, &'static str>(42)
            }
        };

        let result = retry_async(&config, operation, default_classifier).await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_async_success_after_retries() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(1), // Fast for testing
            max_delay: Duration::from_millis(10),
            jitter: JitterStrategy::FullJitter,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let operation = move || {
            let count = call_count_clone.clone();
            async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                if current < 2 {
                    Err("temporary failure")
                } else {
                    Ok(42)
                }
            }
        };

        let result = retry_async(&config, operation, default_classifier).await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_async_all_attempts_fail() {
        let config = RetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            jitter: JitterStrategy::FullJitter,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let operation = move || {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, &'static str>("permanent failure")
            }
        };

        let result = retry_async(&config, operation, default_classifier).await;
        assert_eq!(result.unwrap_err(), "permanent failure");
        assert_eq!(call_count.load(Ordering::SeqCst), 3); // Initial + 2 retries
    }

    #[tokio::test]
    async fn test_retry_async_stops_on_classify_decision() {
        let config = RetryConfig {
            max_attempts: 5,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            jitter: JitterStrategy::FullJitter,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let operation = move || {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, i32>(404) // Return 404 error
            }
        };

        // Classifier that stops on 404 errors
        let classifier = |error: &i32| {
            if *error == 404 {
                RetryDecision::Stop
            } else {
                RetryDecision::Retry
            }
        };

        let result = retry_async(&config, operation, classifier).await;
        assert_eq!(result.unwrap_err(), 404);
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // Should stop immediately
    }

    #[test]
    fn test_jitter_strategy_serialization() {
        // Test that jitter strategies can be serialized/deserialized
        let full_jitter = JitterStrategy::FullJitter;
        let equal_jitter = JitterStrategy::EqualJitter;

        let full_json = serde_json::to_string(&full_jitter).unwrap();
        let equal_json = serde_json::to_string(&equal_jitter).unwrap();

        let deserialized_full: JitterStrategy = serde_json::from_str(&full_json).unwrap();
        let deserialized_equal: JitterStrategy = serde_json::from_str(&equal_json).unwrap();

        assert_eq!(deserialized_full, JitterStrategy::FullJitter);
        assert_eq!(deserialized_equal, JitterStrategy::EqualJitter);
    }

    #[test]
    fn test_retry_config_serialization() {
        let config = RetryConfig::new(
            5,
            Duration::from_millis(250),
            Duration::from_secs(45),
            JitterStrategy::EqualJitter,
        );

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: RetryConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.max_attempts, 5);
        assert_eq!(deserialized.base_delay, Duration::from_millis(250));
        assert_eq!(deserialized.max_delay, Duration::from_secs(45));
        assert_eq!(deserialized.jitter, JitterStrategy::EqualJitter);
    }

    #[test]
    fn test_deterministic_delay_growth_with_seeded_rng() {
        let config = RetryConfig {
            max_attempts: 5,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            jitter: JitterStrategy::FullJitter,
        };

        let seed = 42;

        // Test that delays grow exponentially (before jitter)
        let delay0 = config.calculate_delay_seeded(0, seed);
        let delay1 = config.calculate_delay_seeded(1, seed);
        let delay2 = config.calculate_delay_seeded(2, seed);

        // With the seeded RNG, results should be deterministic
        assert_eq!(delay0, config.calculate_delay_seeded(0, seed));
        assert_eq!(delay1, config.calculate_delay_seeded(1, seed));
        assert_eq!(delay2, config.calculate_delay_seeded(2, seed));

        // Verify that exponential growth is happening (before jitter is applied)
        // For FullJitter, the max possible delay should follow exponential pattern
        assert!(delay0.as_millis() <= 100); // 100 * 2^0 = 100ms max
        assert!(delay1.as_millis() <= 200); // 100 * 2^1 = 200ms max
        assert!(delay2.as_millis() <= 400); // 100 * 2^2 = 400ms max
    }

    #[test]
    fn test_equal_jitter_bounds() {
        let config = RetryConfig {
            max_attempts: 1,
            base_delay: Duration::from_millis(1000),
            max_delay: Duration::from_secs(30),
            jitter: JitterStrategy::EqualJitter,
        };

        let seed = 12345;
        let delay = config.calculate_delay_seeded(0, seed);

        // EqualJitter should be between half and full delay
        let half_delay = 500; // 1000ms / 2
        assert!(delay.as_millis() >= half_delay);
        assert!(delay.as_millis() <= 1000);
    }

    #[test]
    fn test_full_jitter_bounds() {
        let config = RetryConfig {
            max_attempts: 1,
            base_delay: Duration::from_millis(1000),
            max_delay: Duration::from_secs(30),
            jitter: JitterStrategy::FullJitter,
        };

        let seed = 54321;
        let delay = config.calculate_delay_seeded(0, seed);

        // FullJitter should be between 0 and full delay
        assert!(delay.as_millis() <= 1000);
    }
}
