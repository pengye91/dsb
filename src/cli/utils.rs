// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! CLI utility functions

use shell_words::split;

/// Parses a command vector, applying shell word splitting if needed.
///
/// If the vector contains a single element, it's parsed using shell word splitting
/// to support quoted commands. Otherwise, returns the vector as-is.
///
/// # Arguments
///
/// * `command` - A vector of command arguments
///
/// # Returns
///
/// A properly parsed vector of command arguments
///
/// # Examples
///
/// ```
/// use dsb::cli::utils::parse_command_args;
///
/// // Single quoted string - will be parsed
/// let cmd = vec!["sudo /usr/bin/supervisord -c /etc/supervisor/conf.d/supervisord.conf".to_string()];
/// let parsed = parse_command_args(cmd);
/// assert_eq!(parsed, vec!["sudo", "/usr/bin/supervisord", "-c", "/etc/supervisor/conf.d/supervisord.conf"]);
///
/// // Multiple arguments - already split, returned as-is
/// let cmd = vec!["echo".to_string(), "hello".to_string()];
/// let parsed = parse_command_args(cmd);
/// assert_eq!(parsed, vec!["echo", "hello"]);
/// ```
pub fn parse_command_args(command: Vec<String>) -> Vec<String> {
    if command.len() == 1 {
        // Single string - apply shell word splitting
        match split(&command[0]) {
            Ok(parsed) => {
                tracing::debug!("Parsed command string '{:?}' into {:?}", command[0], parsed);
                parsed
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to parse command string {:?}: {}, using as-is",
                    command[0],
                    e
                );
                // If parsing fails, use the original string
                command
            }
        }
    } else {
        // Multiple elements - already split correctly
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command_single_string() {
        let cmd = vec![
            "sudo /usr/bin/supervisord -c /etc/supervisor/conf.d/supervisord.conf".to_string(),
        ];
        let parsed = parse_command_args(cmd);
        assert_eq!(
            parsed,
            vec![
                "sudo",
                "/usr/bin/supervisord",
                "-c",
                "/etc/supervisor/conf.d/supervisord.conf"
            ]
        );
    }

    #[test]
    fn test_parse_command_multiple_args() {
        let cmd = vec![
            "sudo".to_string(),
            "/usr/bin/supervisord".to_string(),
            "-c".to_string(),
            "/etc/supervisor/conf.d/supervisord.conf".to_string(),
        ];
        let parsed = parse_command_args(cmd);
        assert_eq!(
            parsed,
            vec![
                "sudo",
                "/usr/bin/supervisord",
                "-c",
                "/etc/supervisor/conf.d/supervisord.conf"
            ]
        );
    }

    #[test]
    fn test_parse_command_with_quotes() {
        let cmd = vec!["echo 'hello world'".to_string()];
        let parsed = parse_command_args(cmd);
        assert_eq!(parsed, vec!["echo", "hello world"]);
    }

    #[test]
    fn test_parse_command_empty() {
        let cmd = vec!["".to_string()];
        let parsed = parse_command_args(cmd);
        // shell-words returns empty vec for empty string, which is correct
        assert_eq!(parsed, Vec::<String>::new());
    }

    #[test]
    fn test_parse_command_with_escaped_spaces() {
        let cmd = vec!["echo hello\\ world".to_string()];
        let parsed = parse_command_args(cmd);
        assert_eq!(parsed, vec!["echo", "hello world"]);
    }

    #[test]
    fn test_parse_command_malformed_shells_quotes() {
        // Unterminated quote should fail parsing and return as-is
        let cmd = vec!["echo 'hello".to_string()];
        let parsed = parse_command_args(cmd);
        // Should return original command when parsing fails
        assert_eq!(parsed, vec!["echo 'hello"]);
    }

    #[test]
    fn test_parse_command_with_double_quotes() {
        let cmd = vec!["echo \"hello world\"".to_string()];
        let parsed = parse_command_args(cmd);
        assert_eq!(parsed, vec!["echo", "hello world"]);
    }

    #[test]
    fn test_parse_command_mixed_quotes() {
        let cmd = vec!["echo \"it's\" 'a \"test\"'".to_string()];
        let parsed = parse_command_args(cmd);
        assert_eq!(parsed, vec!["echo", "it's", "a \"test\""]);
    }

    #[test]
    fn test_parse_command_with_newlines() {
        let cmd = vec!["echo $'hello\\nworld'".to_string()];
        let parsed = parse_command_args(cmd);
        // shell-words should handle this
        assert!(!parsed.is_empty());
        assert_eq!(parsed[0], "echo");
    }
}
