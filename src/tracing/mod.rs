// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Observability and tracing utilities.
//!
//! This module provides structured tracing support for semioscan operations.

pub(crate) mod spans;

// Note: All span functions are internal (pub(crate)) and not re-exported
