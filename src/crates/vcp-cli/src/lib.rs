// SPDX-License-Identifier: Apache-2.0
//! CLI inputs and presentation. Admission belongs to the canonical owner.
#[cfg(windows)]
pub mod app;
pub mod args;
pub mod backup;
pub mod binding;
pub mod continuation;
#[cfg(windows)]
pub mod control;
pub mod exit_status;
pub mod history;
pub mod input;
pub mod jsonl;
pub mod outcome;
pub mod output;
pub mod questions;
#[cfg(windows)]
pub mod rebind;
pub mod selection;
#[cfg(windows)]
pub mod session;
pub mod settings;
pub mod storage;
pub mod terminal;
