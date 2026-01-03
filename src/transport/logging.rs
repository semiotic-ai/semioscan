// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Tower-based logging layer for Alloy RPC providers.
//!
//! This module implements a logging layer that uses `tracing` to record
//! RPC request/response information for debugging and observability.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use alloy_json_rpc::{RequestPacket, ResponsePacket};
use alloy_transport::TransportError;
use tower::Layer;
use tracing::{debug, trace, warn, Span};

/// A Tower layer that adds logging/tracing to RPC requests.
///
/// This layer wraps each RPC request in a tracing span and logs
/// timing information, request details, and any errors that occur.
///
/// # Example
///
/// ```rust,ignore
/// use semioscan::transport::LoggingLayer;
/// use alloy_rpc_client::ClientBuilder;
///
/// let client = ClientBuilder::default()
///     .layer(LoggingLayer::new())
///     .http(rpc_url);
/// ```
#[derive(Clone, Debug, Default)]
pub struct LoggingLayer {
    /// Whether to log request payloads (can be verbose)
    log_requests: bool,
    /// Whether to log response payloads (can be verbose)
    log_responses: bool,
}

impl LoggingLayer {
    /// Creates a new logging layer with default settings.
    ///
    /// By default, only timing and errors are logged.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enables logging of request payloads.
    ///
    /// Warning: This can be verbose for large requests.
    pub fn with_request_logging(mut self) -> Self {
        self.log_requests = true;
        self
    }

    /// Enables logging of response payloads.
    ///
    /// Warning: This can be verbose for large responses.
    pub fn with_response_logging(mut self) -> Self {
        self.log_responses = true;
        self
    }

    /// Enables logging of both request and response payloads.
    pub fn verbose(mut self) -> Self {
        self.log_requests = true;
        self.log_responses = true;
        self
    }
}

impl<S> Layer<S> for LoggingLayer {
    type Service = LoggingService<S>;

    fn layer(&self, service: S) -> Self::Service {
        LoggingService {
            service,
            log_requests: self.log_requests,
            log_responses: self.log_responses,
        }
    }
}

/// A Tower service that logs RPC requests and responses.
#[derive(Clone, Debug)]
pub struct LoggingService<S> {
    service: S,
    log_requests: bool,
    log_responses: bool,
}

impl<S> tower::Service<RequestPacket> for LoggingService<S>
where
    S: tower::Service<RequestPacket, Response = ResponsePacket, Error = TransportError>
        + Clone
        + Send
        + 'static,
    S::Future: Send,
{
    type Response = ResponsePacket;
    type Error = TransportError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: RequestPacket) -> Self::Future {
        let log_requests = self.log_requests;
        let log_responses = self.log_responses;
        let mut service = self.service.clone();

        // Extract method name from the request for logging
        let method = extract_method(&request);

        Box::pin(async move {
            let start = Instant::now();

            // Create a span for this RPC call
            let span = tracing::info_span!(
                "rpc_call",
                method = %method,
                duration_ms = tracing::field::Empty,
            );

            let _guard = span.enter();

            if log_requests {
                trace!(request = ?request, "RPC request");
            } else {
                debug!("RPC request: {method}");
            }

            let result = service.call(request).await;
            let duration = start.elapsed();

            // Record duration in the span
            Span::current().record("duration_ms", duration.as_millis() as u64);

            match &result {
                Ok(response) => {
                    if log_responses {
                        trace!(
                            response = ?response,
                            duration_ms = %duration.as_millis(),
                            "RPC response"
                        );
                    } else {
                        debug!(
                            duration_ms = %duration.as_millis(),
                            "RPC response: {method}"
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        duration_ms = %duration.as_millis(),
                        "RPC error: {method}"
                    );
                }
            }

            result
        })
    }
}

/// Extract the RPC method name from a request packet.
fn extract_method(request: &RequestPacket) -> String {
    match request {
        RequestPacket::Single(req) => req.method().to_string(),
        RequestPacket::Batch(reqs) => {
            if reqs.is_empty() {
                "batch(empty)".to_string()
            } else if reqs.len() == 1 {
                reqs[0].method().to_string()
            } else {
                format!("batch({} calls)", reqs.len())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logging_layer_default() {
        let layer = LoggingLayer::new();
        assert!(!layer.log_requests);
        assert!(!layer.log_responses);
    }

    #[test]
    fn test_logging_layer_with_request_logging() {
        let layer = LoggingLayer::new().with_request_logging();
        assert!(layer.log_requests);
        assert!(!layer.log_responses);
    }

    #[test]
    fn test_logging_layer_verbose() {
        let layer = LoggingLayer::new().verbose();
        assert!(layer.log_requests);
        assert!(layer.log_responses);
    }
}
