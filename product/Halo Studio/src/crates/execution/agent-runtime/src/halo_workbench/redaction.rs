//! Halo Workbench Runtime sanitization and validation helpers: redaction
//! machinery, input validation and request fingerprints.

use std::path::PathBuf;


use halo_runtime_ports::WorkbenchTaskBaseline;
use sha2::{Digest, Sha256};
use super::vocabulary::*;

pub(super) fn validate_workspace_input(
    workspace: &HaloWorkbenchWorkspaceInput,
) -> Result<(), HaloWorkbenchError> {
    if workspace.workspace_id.trim().is_empty() {
        return Err(HaloWorkbenchError::invalid_request(
            "A workspace identifier is required",
        ));
    }
    if workspace.display_name.trim().is_empty() {
        return Err(HaloWorkbenchError::invalid_request(
            "A workspace display name is required",
        ));
    }
    if workspace.root_path.as_os_str().is_empty() {
        return Err(HaloWorkbenchError::invalid_request(
            "A workspace root is required",
        ));
    }
    Ok(())
}

pub(super) fn validate_workspace_confirmation(
    workspace_id: &str,
    root_path: &PathBuf,
) -> Result<(), HaloWorkbenchError> {
    if workspace_id.trim().is_empty() || root_path.as_os_str().is_empty() {
        return Err(HaloWorkbenchError::invalid_request(
            "A workspace identity and canonical root are required for managed confirmation",
        ));
    }
    if workspace_id.chars().any(char::is_control)
        || root_path.to_string_lossy().chars().any(char::is_control)
    {
        return Err(HaloWorkbenchError::invalid_request(
            "The managed workspace confirmation contains invalid characters",
        ));
    }
    Ok(())
}


pub(super) fn validate_task_baseline(baseline: &WorkbenchTaskBaseline) -> Result<(), ()> {
    if baseline.head.trim().is_empty()
        || baseline.canonical_root.as_os_str().is_empty()
        || baseline.captured_at_ms < 0
        || baseline.existing_changed_files.len() > MAX_BASELINE_CHANGED_FILES
        || baseline.working_tree_fingerprint.len() != BASELINE_FINGERPRINT_HEX_LENGTH
        || !baseline
            .working_tree_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || baseline.existing_changed_files.iter().any(|path| {
            path.trim().is_empty()
                || path.len() > MAX_PUBLIC_LABEL_BYTES * 8
                || path.chars().any(char::is_control)
        })
    {
        return Err(());
    }
    Ok(())
}

pub(super) fn redact_halo_text(value: &str, max_bytes: usize) -> String {
    let mut redacted = value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .collect::<String>();
    for header in ["authorization", "cookie"] {
        redacted = redact_halo_header_values(&redacted, header);
    }
    for prefix in ["sk-", "sk_", "ghp_", "github_pat_", "xoxb-", "AIza"] {
        redacted = redact_prefixed_halo_token(&redacted, prefix);
    }
    redacted = redact_halo_literal_value(&redacted, "bearer ");
    for name in [
        "api-key",
        "api_key",
        "secret",
        "token",
        "password",
        "sessionid",
        "entryid",
        "toolcallid",
        "session_id",
        "entry_id",
        "tool_call_id",
    ] {
        redacted = redact_halo_named_values(&redacted, name);
    }
    truncate_utf8(&redacted, max_bytes)
}

pub(super) fn redact_halo_header_values(value: &str, header: &str) -> String {
    let mut redacted = value.to_string();
    let mut cursor = 0;
    while cursor < redacted.len() {
        let Some(start) = find_halo_named_marker(&redacted, header, cursor) else {
            break;
        };
        let mut delimiter = start + header.len();
        if redacted[delimiter..].starts_with('"') || redacted[delimiter..].starts_with('\'') {
            delimiter += 1;
        }
        delimiter = skip_halo_horizontal_whitespace(&redacted, delimiter);
        if !redacted[delimiter..].starts_with(':') && !redacted[delimiter..].starts_with('=') {
            cursor = delimiter;
            continue;
        }
        let value_start = skip_halo_horizontal_whitespace(&redacted, delimiter + 1);
        let value_end = halo_header_value_end(&redacted, value_start);
        if value_start == value_end {
            cursor = value_start;
            continue;
        }
        redacted.replace_range(value_start..value_end, "[redacted]");
        cursor = value_start + "[redacted]".len();
    }
    redacted
}

pub(super) fn redact_halo_named_values(value: &str, name: &str) -> String {
    let mut redacted = value.to_string();
    let mut cursor = 0;
    while cursor < redacted.len() {
        let Some(start) = find_halo_named_marker(&redacted, name, cursor) else {
            break;
        };
        let mut delimiter = start + name.len();
        if redacted[delimiter..].starts_with('"') || redacted[delimiter..].starts_with('\'') {
            delimiter += 1;
        }
        delimiter = skip_halo_horizontal_whitespace(&redacted, delimiter);
        if !redacted[delimiter..].starts_with(':') && !redacted[delimiter..].starts_with('=') {
            cursor = delimiter;
            continue;
        }
        let mut value_start = skip_halo_horizontal_whitespace(&redacted, delimiter + 1);
        let quote = redacted[value_start..]
            .chars()
            .next()
            .filter(|character| matches!(character, '"' | '\'' | '`'));
        if let Some(quote) = quote {
            value_start += quote.len_utf8();
            let value_end = halo_quoted_value_end(&redacted, value_start, quote);
            if value_start != value_end {
                redacted.replace_range(value_start..value_end, "[redacted]");
                cursor = value_start + "[redacted]".len();
                continue;
            }
        } else {
            let value_end = halo_token_value_end(&redacted, value_start);
            if value_start != value_end {
                redacted.replace_range(value_start..value_end, "[redacted]");
                cursor = value_start + "[redacted]".len();
                continue;
            }
        }
        cursor = value_start;
    }
    redacted
}

pub(super) fn redact_halo_literal_value(value: &str, marker: &str) -> String {
    let mut redacted = value.to_string();
    let mut cursor = 0;
    while cursor < redacted.len() {
        let lower = redacted[cursor..].to_ascii_lowercase();
        let Some(relative) = lower.find(marker) else {
            break;
        };
        let value_start = cursor + relative + marker.len();
        let value_end = halo_token_value_end(&redacted, value_start);
        if value_start == value_end {
            cursor = value_start;
            continue;
        }
        redacted.replace_range(value_start..value_end, "[redacted]");
        cursor = value_start + "[redacted]".len();
    }
    redacted
}

pub(super) fn find_halo_named_marker(value: &str, name: &str, mut cursor: usize) -> Option<usize> {
    while cursor < value.len() {
        let lower = value[cursor..].to_ascii_lowercase();
        let relative = lower.find(name)?;
        let start = cursor + relative;
        let end = start + name.len();
        if halo_identifier_boundary(value, start, end) {
            return Some(start);
        }
        cursor = end;
    }
    None
}

pub(super) fn halo_identifier_boundary(value: &str, start: usize, end: usize) -> bool {
    let before = value[..start].chars().next_back();
    let after = value[end..].chars().next();
    !before.is_some_and(is_halo_identifier_character)
        && !after.is_some_and(is_halo_identifier_character)
}

pub(super) fn is_halo_identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

pub(super) fn skip_halo_horizontal_whitespace(value: &str, mut cursor: usize) -> usize {
    while let Some(character) = value[cursor..].chars().next() {
        if !matches!(character, ' ' | '\t') {
            break;
        }
        cursor += character.len_utf8();
    }
    cursor
}

pub(super) fn halo_header_value_end(value: &str, value_start: usize) -> usize {
    for (offset, character) in value[value_start..].char_indices() {
        let cursor = value_start + offset;
        if matches!(character, '\n' | '\r') {
            return cursor;
        }
        if cursor > value_start
            && value[..cursor]
                .chars()
                .next_back()
                .is_some_and(|previous| matches!(previous, ' ' | '\t'))
            && is_inline_halo_sensitive_key(value, cursor)
        {
            let mut boundary = cursor;
            while boundary > value_start
                && value[..boundary]
                    .chars()
                    .next_back()
                    .is_some_and(|previous| matches!(previous, ' ' | '\t'))
            {
                boundary -= value[..boundary]
                    .chars()
                    .next_back()
                    .expect("boundary has a preceding character")
                    .len_utf8();
            }
            return boundary;
        }
    }
    value.len()
}

pub(super) fn is_inline_halo_sensitive_key(value: &str, cursor: usize) -> bool {
    [
        "authorization",
        "cookie",
        "api-key",
        "api_key",
        "secret",
        "token",
        "password",
        "sessionid",
        "entryid",
        "toolcallid",
        "session_id",
        "entry_id",
        "tool_call_id",
    ]
    .into_iter()
    .any(|name| {
        find_halo_named_marker(value, name, cursor) == Some(cursor)
            && halo_named_marker_has_value_delimiter(value, cursor, name)
    })
}

pub(super) fn halo_named_marker_has_value_delimiter(value: &str, start: usize, name: &str) -> bool {
    let mut cursor = start + name.len();
    if value[cursor..].starts_with('"') || value[cursor..].starts_with('\'') {
        cursor += 1;
    }
    cursor = skip_halo_horizontal_whitespace(value, cursor);
    value[cursor..].starts_with(':') || value[cursor..].starts_with('=')
}

pub(super) fn halo_quoted_value_end(value: &str, value_start: usize, quote: char) -> usize {
    let mut escaped = false;
    for (offset, character) in value[value_start..].char_indices() {
        if character == quote && !escaped {
            return value_start + offset;
        }
        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    value.len()
}

pub(super) fn halo_token_value_end(value: &str, value_start: usize) -> usize {
    value[value_start..]
        .char_indices()
        .find(|(_, character)| {
            character.is_whitespace()
                || matches!(character, '"' | '\'' | '`' | ',' | ';' | '}' | ']')
        })
        .map(|(offset, _)| value_start + offset)
        .unwrap_or(value.len())
}

pub(super) fn redact_prefixed_halo_token(value: &str, prefix: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut cursor = 0;
    while let Some(relative) = value[cursor..].find(prefix) {
        let start = cursor + relative;
        result.push_str(&value[cursor..start]);
        let end = value[start..]
            .char_indices()
            .find(|(_, character)| {
                character.is_whitespace() || matches!(character, '"' | '\'' | '`' | ',' | ';')
            })
            .map(|(offset, _)| start + offset)
            .unwrap_or(value.len());
        result.push_str("[redacted]");
        cursor = end;
        if cursor >= value.len() {
            break;
        }
    }
    result.push_str(&value[cursor..]);
    result
}

pub(super) fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

pub(super) fn validate_operation_decision(
    decision: &HaloWorkbenchOperationDecision,
) -> Result<(), HaloWorkbenchError> {
    match decision {
        HaloWorkbenchOperationDecision::AllowOnce | HaloWorkbenchOperationDecision::Deny => Ok(()),
    }
}

pub(super) fn validate_user_input(content: &str) -> Result<(), HaloWorkbenchError> {
    if content.trim().is_empty() {
        return Err(HaloWorkbenchError::invalid_request(
            "Non-empty user input is required",
        ));
    }
    Ok(())
}

pub(super) fn validate_task_id(task_id: &str) -> Result<(), HaloWorkbenchError> {
    if task_id.trim().is_empty()
        || task_id.len() > 256
        || task_id
            .chars()
            .any(|character| character.is_control() || character == '\\')
    {
        return Err(HaloWorkbenchError::invalid_request(
            "A safe, non-empty task identifier is required",
        ));
    }
    Ok(())
}

pub(super) fn request_fingerprint(intent: &HaloWorkbenchIntent) -> Result<[u8; 32], HaloWorkbenchError> {
    let encoded = serde_json::to_vec(intent).map_err(|_| {
        HaloWorkbenchError::new(
            "runtime_internal",
            "The Workbench intent could not be fingerprinted",
            "retry",
        )
    })?;
    Ok(Sha256::digest(encoded).into())
}
