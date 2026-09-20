// SPDX-License-Identifier: Apache-2.0
//! Bounded sequential MCP 2025-11-25 stdio profile. Data and bytes only: the host
//! owns transport, current authority, source fences, deadlines and durable intent.
//! No automatic replay, reconnect, callback execution or SDK service runtime.
pub mod client;
pub mod identity;
pub mod registration;
pub mod schema;
