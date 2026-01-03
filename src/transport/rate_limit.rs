// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Tower-based rate limiting layer for Alloy RPC providers.
//!
//! This module implements a token bucket rate limiter as a Tower `Layer`
//! that can be composed with Alloy's transport system.

use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use tokio::sync::Mutex;
use tower::Layer;

/// A Tower layer that applies rate limiting to requests.
///
/// This layer uses a token bucket algorithm to limit the rate of requests.
/// Tokens are replenished at a fixed rate, and each request consumes one token.
/// If no tokens are available, the request waits until a token becomes available.
///
/// # Example
///
/// ```rust,ignore
/// use semioscan::transport::RateLimitLayer;
/// use alloy_rpc_client::ClientBuilder;
/// use std::time::Duration;
///
/// // Rate limit to 10 requests per second
/// let layer = RateLimitLayer::new(10, Duration::from_secs(1));
///
/// let client = ClientBuilder::default()
///     .layer(layer)
///     .http(rpc_url);
/// ```
#[derive(Clone, Debug)]
pub struct RateLimitLayer {
    state: Arc<Mutex<RateLimitState>>,
}

impl RateLimitLayer {
    /// Creates a new rate limit layer.
    ///
    /// # Arguments
    ///
    /// * `requests` - Maximum number of requests allowed in the given period
    /// * `period` - The time period for the rate limit
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::transport::RateLimitLayer;
    /// use std::time::Duration;
    ///
    /// // Allow 10 requests per second
    /// let layer = RateLimitLayer::new(10, Duration::from_secs(1));
    ///
    /// // Allow 100 requests per minute
    /// let layer = RateLimitLayer::new(100, Duration::from_secs(60));
    /// ```
    pub fn new(requests: u32, period: Duration) -> Self {
        Self {
            state: Arc::new(Mutex::new(RateLimitState::new(requests, period))),
        }
    }

    /// Creates a rate limit layer from requests per second.
    ///
    /// This is a convenience constructor for common rate limiting scenarios.
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::transport::RateLimitLayer;
    ///
    /// // 25 requests per second
    /// let layer = RateLimitLayer::per_second(25);
    /// ```
    pub fn per_second(requests: u32) -> Self {
        Self::new(requests, Duration::from_secs(1))
    }

    /// Creates a rate limit layer with a minimum delay between requests.
    ///
    /// This is useful when you want to ensure a fixed delay between
    /// consecutive requests rather than allowing bursts.
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::transport::RateLimitLayer;
    /// use std::time::Duration;
    ///
    /// // At least 100ms between requests (max 10 req/s)
    /// let layer = RateLimitLayer::with_min_delay(Duration::from_millis(100));
    /// ```
    pub fn with_min_delay(delay: Duration) -> Self {
        Self::new(1, delay)
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, service: S) -> Self::Service {
        RateLimitService {
            service,
            state: self.state.clone(),
        }
    }
}

/// Internal state for the token bucket rate limiter.
#[derive(Debug)]
struct RateLimitState {
    /// Maximum number of tokens (requests) available
    capacity: u32,
    /// Current number of available tokens
    tokens: f64,
    /// Token replenishment rate (tokens per nanosecond)
    refill_rate: f64,
    /// Last time tokens were refilled
    last_refill: Instant,
}

impl RateLimitState {
    fn new(requests: u32, period: Duration) -> Self {
        let refill_rate = requests as f64 / period.as_nanos() as f64;
        Self {
            capacity: requests,
            tokens: requests as f64,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Try to acquire a token, returning the wait time if not available.
    fn try_acquire(&mut self) -> Option<Duration> {
        self.refill();

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            None
        } else {
            // Calculate how long to wait for 1 token
            let needed = 1.0 - self.tokens;
            let wait_nanos = needed / self.refill_rate;
            Some(Duration::from_nanos(wait_nanos as u64))
        }
    }

    /// Refill tokens based on elapsed time.
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let new_tokens = elapsed.as_nanos() as f64 * self.refill_rate;

        self.tokens = (self.tokens + new_tokens).min(self.capacity as f64);
        self.last_refill = now;
    }
}

/// A Tower service that applies rate limiting to requests.
///
/// This service wraps an inner service and ensures that requests
/// are made at a controlled rate using a token bucket algorithm.
#[derive(Clone, Debug)]
pub struct RateLimitService<S> {
    service: S,
    state: Arc<Mutex<RateLimitState>>,
}

impl<S, Request> tower::Service<Request> for RateLimitService<S>
where
    S: tower::Service<Request> + Clone + Send + 'static,
    S::Future: Send,
    Request: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let state = self.state.clone();
        let mut service = self.service.clone();

        Box::pin(async move {
            // Acquire a token, waiting if necessary
            loop {
                let wait_time = {
                    let mut state = state.lock().await;
                    state.try_acquire()
                };

                match wait_time {
                    None => break,
                    Some(duration) => {
                        tokio::time::sleep(duration).await;
                    }
                }
            }

            service.call(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_rate_limit_state_immediate_acquire() {
        let mut state = RateLimitState::new(10, Duration::from_secs(1));

        // Should be able to acquire immediately
        assert!(state.try_acquire().is_none());
        assert!(state.try_acquire().is_none());
    }

    #[tokio::test]
    async fn test_rate_limit_state_exhaustion() {
        let mut state = RateLimitState::new(2, Duration::from_secs(1));

        // Exhaust all tokens
        assert!(state.try_acquire().is_none());
        assert!(state.try_acquire().is_none());

        // Third request should require waiting
        let wait = state.try_acquire();
        assert!(wait.is_some());
    }

    #[tokio::test]
    async fn test_rate_limit_state_refill() {
        let mut state = RateLimitState::new(10, Duration::from_secs(1));

        // Exhaust all tokens
        for _ in 0..10 {
            state.try_acquire();
        }

        // Wait for refill
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Should have some tokens now (approximately 2)
        assert!(state.try_acquire().is_none());
    }

    #[tokio::test]
    async fn test_rate_limit_layer_construction() {
        let layer = RateLimitLayer::new(10, Duration::from_secs(1));
        assert!(layer.state.lock().await.capacity == 10);
    }

    #[tokio::test]
    async fn test_rate_limit_per_second() {
        let layer = RateLimitLayer::per_second(25);
        assert!(layer.state.lock().await.capacity == 25);
    }

    #[tokio::test]
    async fn test_rate_limit_enforces_rate() {
        // Service that returns immediately
        #[derive(Clone)]
        struct InstantService;

        impl tower::Service<()> for InstantService {
            type Response = ();
            type Error = std::convert::Infallible;
            type Future = std::future::Ready<Result<(), std::convert::Infallible>>;

            fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, _req: ()) -> Self::Future {
                std::future::ready(Ok(()))
            }
        }

        // 5 requests per second means ~200ms between requests
        let layer = RateLimitLayer::per_second(5);
        let mut service = layer.layer(InstantService);

        let start = Instant::now();

        // Make 6 requests (first 5 should be instant, 6th should wait)
        for _ in 0..6 {
            tower::Service::call(&mut service, ()).await.unwrap();
        }

        let elapsed = start.elapsed();
        // Should take at least 200ms for the 6th request
        assert!(elapsed >= Duration::from_millis(180));
    }
}
