// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie

mod activities;
mod api_keys;
mod file_transfer;
mod health_config;
mod images;
mod parsers;
mod runner;
mod sandbox;
mod server;
mod session_tokens;
mod ssh_sessions;
mod static_files;
#[cfg(test)]
mod tests;
mod tools;
mod types;
mod web;

pub use runner::run_cli;
pub use types::{
    ActivitiesCommands, ApiKeyCommands, Cli, Commands, ImagesCommands, OutputFormat,
    SessionTokenCommands, SshSessionCommands, StaticCommands, WebCommands, WebFetchFormat,
    WebSearchEngine,
};

#[cfg(test)]
mod execution_tests;
