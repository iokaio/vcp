// SPDX-License-Identifier: Apache-2.0
//! CLI inputs and presentation. Admission belongs to the canonical owner.
#[cfg(windows)]
pub mod app;
pub mod args;
#[cfg(windows)]
pub mod control;
pub mod exit_status;
pub mod input;
pub mod jsonl;
pub mod outcome;
pub mod output;
#[cfg(windows)]
pub mod session;
pub mod settings;
