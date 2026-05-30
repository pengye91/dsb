// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie

mod types;
mod parsers;
mod sandbox;
mod activities;
mod api_keys;
mod health_config;
mod file_transfer;
mod tools;
mod web;
mod images;
mod static_files;
mod session_tokens;
mod ssh_sessions;
mod server;
mod runner;
#[cfg(test)]
mod tests;

pub use types::{
    Cli, Commands, OutputFormat, WebFetchFormat, WebSearchEngine,
    ActivitiesCommands, ApiKeyCommands, ImagesCommands, StaticCommands,
    SessionTokenCommands, SshSessionCommands, WebCommands,
};
pub use runner::run_cli;

#[cfg(test)]
mod execution_tests;
