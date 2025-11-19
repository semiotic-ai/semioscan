//! Observability and tracing utilities.
//!
//! This module provides structured tracing support for semioscan operations.

pub(crate) mod spans;

// Note: All span functions are internal (pub(crate)) and not re-exported
