//! Maps raw OS, database, keychain, and command errors to user-facing copy.
//!
//! The raw message is logged so the technical detail stays available in the
//! Rust logs while the UI only receives a short, actionable sentence.

/// Ordered raw-pattern to friendly-message map. Order matters: earlier rules
/// win, so specific errors are matched before broader ones like `database`.
const RULES: &[(&[&str], &str)] = &[
    (
        &[
            "no such file or directory",
            "cannot find the file",
            "the system cannot find the file",
            "os error 2",
            "enoent",
        ],
        "A file or folder couldn't be found. Check the executable and working directory.",
    ),
    (
        &[
            "permission denied",
            "access is denied",
            "operation not permitted",
            "os error 13",
            "os error 1",
            "os error 5",
            "eacces",
            "eperm",
        ],
        "ProcessHub doesn't have permission to do that. Check the file's permissions and try again.",
    ),
    (
        &["text file busy", "os error 26", "etxtbsy"],
        "The executable is still in use. Try again in a moment.",
    ),
    (
        &[
            "exec format error",
            "os error 8",
            "enoexec",
            "not executable",
        ],
        "That file can't be run directly. Choose an app, script, or executable.",
    ),
    (
        &[
            "address already in use",
            "os error 48",
            "os error 98",
            "eaddrinuse",
        ],
        "That port is already in use. Stop the other process or choose a different port.",
    ),
    (
        &[
            "connection refused",
            "os error 61",
            "os error 111",
            "econnrefused",
        ],
        "The service refused the connection. It may still be starting.",
    ),
    (
        &["connection reset", "econnreset"],
        "The connection closed unexpectedly. Try again.",
    ),
    (
        &["timed out", "timeout", "etimedout"],
        "The operation timed out. Try again.",
    ),
    (
        &["no space left", "enospc", "os error 28"],
        "There isn't enough disk space to finish that.",
    ),
    (
        &[
            "is a directory",
            "not a directory",
            "os error 21",
            "os error 20",
            "eisdir",
            "enotdir",
        ],
        "A path points to the wrong kind of file. Check the executable and working directory.",
    ),
    (
        &["too many open files", "emfile"],
        "ProcessHub has too many files open. Restart it and try again.",
    ),
    (
        &["database is locked", "sqlite_busy"],
        "ProcessHub is busy saving changes. Try again.",
    ),
    (
        &["database", "sqlite", "rusqlite"],
        "ProcessHub couldn't save that change. Try again.",
    ),
    (
        &["keychain", "keyring", "secret service", "os error -25300"],
        "The secret couldn't be saved to the system keychain. Try again.",
    ),
    (
        &["lock poisoned", "poisoned"],
        "ProcessHub hit an internal error. Restart the app and try again.",
    ),
    (
        &["tcp check", "http check", "health check failed"],
        "The health check couldn't reach the service. It may still be starting.",
    ),
];

/// Matches `os error N` as a whole error code so `os error 21` is not treated
/// as `os error 2` and `os error 111` is not treated as `os error 1`.
fn contains_needle(message: &str, needle: &str) -> bool {
    let check_boundary = needle.starts_with("os error ");
    let mut offset = 0;
    while let Some(index) = message[offset..].find(needle) {
        let end = offset + index + needle.len();
        let next_is_digit = message[end..]
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit());
        if !check_boundary || !next_is_digit {
            return true;
        }
        offset = end;
    }
    false
}

fn has_technical_marker(message: &str, lower: &str) -> bool {
    const MARKERS: &[&str] = &[
        "os error",
        "errno",
        "sqlite",
        "rusqlite",
        "keyring",
        "reqwest",
        "`",
        "\n",
        "\r",
        "panicked",
        "unwrap",
        "rust_backtrace",
        " at src/",
        "://",
    ];

    message.chars().count() > 220 || MARKERS.iter().any(|marker| lower.contains(*marker))
}

/// Converts a raw error message into user-facing copy.
///
/// Rules are matched case-insensitively in the order defined by the canonical
/// error map. Messages that are already short, friendly sentences pass through
/// unchanged; messages that still look technical fall back to `fallback`.
pub fn friendly_error(message: &str, fallback: &str) -> String {
    let original = message.trim();
    let message = original
        .strip_prefix("Error: ")
        .or_else(|| original.strip_prefix("error: "))
        .unwrap_or(original)
        .trim();
    let lower = message.to_lowercase();

    for (needles, copy) in RULES {
        if needles
            .iter()
            .any(|needle| contains_needle(&lower, needle))
        {
            log::warn!("event=raw_error original={original} friendly={copy}");
            return (*copy).to_string();
        }
    }

    if message.is_empty() || has_technical_marker(message, &lower) {
        log::warn!("event=raw_error original={original} friendly={fallback}");
        return fallback.to_string();
    }

    message.to_string()
}

#[cfg(test)]
mod tests {
    use super::friendly_error;

    const FALLBACK: &str = "Something went wrong. Try again.";

    #[test]
    fn enoent_gets_missing_path_copy() {
        assert_eq!(
            friendly_error("No such file or directory (os error 2)", FALLBACK),
            "A file or folder couldn't be found. Check the executable and working directory.",
        );
    }

    #[test]
    fn eacces_gets_permission_copy() {
        assert_eq!(
            friendly_error("Permission denied (os error 13)", FALLBACK),
            "ProcessHub doesn't have permission to do that. Check the file's permissions and try again.",
        );
    }

    #[test]
    fn eaddrinuse_gets_port_copy() {
        assert_eq!(
            friendly_error("Address already in use (os error 48)", FALLBACK),
            "That port is already in use. Stop the other process or choose a different port.",
        );
    }

    #[test]
    fn econnrefused_gets_connection_copy() {
        assert_eq!(
            friendly_error("Connection refused (os error 61)", FALLBACK),
            "The service refused the connection. It may still be starting.",
        );
    }

    #[test]
    fn timeout_gets_timeout_copy() {
        assert_eq!(
            friendly_error("request timed out after 30s", FALLBACK),
            "The operation timed out. Try again.",
        );
        assert_eq!(
            friendly_error("TCP check timed out", FALLBACK),
            "The operation timed out. Try again.",
        );
    }

    #[test]
    fn sqlite_locked_gets_busy_copy() {
        assert_eq!(
            friendly_error("database is locked", FALLBACK),
            "ProcessHub is busy saving changes. Try again.",
        );
    }

    #[test]
    fn keychain_errors_get_secret_copy() {
        assert_eq!(
            friendly_error("failed to access the system keychain", FALLBACK),
            "The secret couldn't be saved to the system keychain. Try again.",
        );
    }

    #[test]
    fn poisoned_locks_get_internal_error_copy() {
        assert_eq!(
            friendly_error("Process registry lock poisoned", FALLBACK),
            "ProcessHub hit an internal error. Restart the app and try again.",
        );
    }

    #[test]
    fn friendly_validation_messages_are_preserved() {
        assert_eq!(
            friendly_error("Executable is required", FALLBACK),
            "Executable is required",
        );
        assert_eq!(
            friendly_error("Service not found", FALLBACK),
            "Service not found",
        );
        assert_eq!(
            friendly_error(
                "Secret API_KEY is not set. Edit the service to set it.",
                FALLBACK,
            ),
            "Secret API_KEY is not set. Edit the service to set it.",
        );
    }

    #[test]
    fn technical_messages_without_a_rule_use_the_fallback() {
        assert_eq!(
            friendly_error("failed to run `fixture` with exit status 1", FALLBACK),
            FALLBACK,
        );
        assert_eq!(
            friendly_error(
                "error sending request for url (https://example.com/release.json)",
                FALLBACK,
            ),
            FALLBACK,
        );
    }

    #[test]
    fn leading_error_prefix_is_stripped() {
        assert_eq!(
            friendly_error("Error: database is locked", FALLBACK),
            "ProcessHub is busy saving changes. Try again.",
        );
        assert_eq!(
            friendly_error("error: Permission denied", FALLBACK),
            "ProcessHub doesn't have permission to do that. Check the file's permissions and try again.",
        );
    }

    #[test]
    fn os_error_codes_match_as_whole_numbers() {
        assert_eq!(
            friendly_error("Not a directory (os error 20)", FALLBACK),
            "A path points to the wrong kind of file. Check the executable and working directory.",
        );
        assert_eq!(
            friendly_error("Is a directory (os error 21)", FALLBACK),
            "A path points to the wrong kind of file. Check the executable and working directory.",
        );
        assert_eq!(
            friendly_error("Connection refused (os error 111)", FALLBACK),
            "The service refused the connection. It may still be starting.",
        );
    }
}
