// SPDX-License-Identifier: Apache-2.0
//! CLI inputs and presentation. Admission belongs to the canonical owner.
pub mod agents_view;
#[cfg(windows)]
pub mod app;
pub mod args;
pub mod backup;
#[cfg(windows)]
pub mod backup_triggers;
pub mod binding;
#[cfg(windows)]
pub mod config_import;
pub mod console_secret;
pub mod continuation;
#[cfg(windows)]
pub mod control;
pub mod credential;
#[cfg(windows)]
pub mod decision;
#[cfg(windows)]
pub mod delegation;
pub mod disk_space;
pub mod doctor;
#[cfg(windows)]
pub mod execution;
#[cfg(windows)]
pub(crate) mod execution_profile;
pub mod exit_status;
pub mod history;
pub mod input;
pub mod inspection_bundle;
#[cfg(windows)]
pub mod installation;
pub mod jsonl;
#[cfg(windows)]
pub mod local;
pub mod mcp;
pub mod memory;
pub mod model_preferences;
pub mod onboarding;
pub mod optimize;
pub mod outcome;
pub mod output;
pub mod provider_setup;
#[cfg(windows)]
pub(crate) mod provider_reconciliation;
pub mod questions;
#[cfg(windows)]
pub mod rebind;
#[cfg(windows)]
pub mod restore;
pub mod selection;
#[cfg(windows)]
pub mod session;
pub mod settings;
pub mod setup_wizard;
pub mod skills;
pub mod storage;
pub mod terminal;
#[cfg(windows)]
pub mod workspace_trust;
