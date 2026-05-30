// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Test fixtures and utilities

use uuid::Uuid;

/// Get a fixed test sandbox ID (UUID)
pub fn test_sandbox_id() -> Uuid {
    Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_sandbox_id() {
        let id = test_sandbox_id();
        assert_eq!(id.to_string(), "123e4567-e89b-12d3-a456-426614174000");
    }
}
