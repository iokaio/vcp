// SPDX-License-Identifier: Apache-2.0
//! CLI inputs and presentation. Admission belongs to the canonical owner.
#[cfg(windows)]
pub mod app;
pub mod args;
pub mod binding;
pub mod continuation;
#[cfg(windows)]
pub mod control;
pub mod exit_status;
pub mod input;
pub mod history;
pub mod jsonl;
pub mod outcome;
pub mod output;
pub mod questions;
#[cfg(windows)]
pub mod rebind;
#[cfg(windows)]
pub mod session;
pub mod settings;
pub mod terminal;
