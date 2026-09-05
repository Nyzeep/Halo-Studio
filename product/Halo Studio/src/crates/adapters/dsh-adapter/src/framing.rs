//! Strict LF-delimited ndjson framing for the DSH ACP wire (ADR-0078).
//!
//! The DeepSeek Harness ACP and SDK profiles both speak newline-delimited
//! JSON-RPC over stdio (protocol research 2026-09-05, section 2): every frame
//! is one `JSON.stringify(message) + '\n'` record and malformed lines are
//! silently ignored by the upstream transport. Halo keeps
//! the same strict framing contract as the pi adapter: LF-only records, one
//! tolerated trailing CR, never splitting on CR, U+2028, or U+2029.

use serde_json::Value;

pub(crate) fn encode_jsonl<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let mut encoded = serde_json::to_vec(value)?;
    encoded.push(b'\n');
    Ok(encoded)
}

/// Decode exactly one record after its LF delimiter has been removed.
pub(crate) fn decode_jsonl_record(record: &[u8]) -> Result<Value, String> {
    if record.contains(&b'\n') {
        return Err("DSH record contains an embedded LF".to_string());
    }

    let record = record.strip_suffix(b"\r").unwrap_or(record);
    if record.is_empty() {
        return Err("DSH record is empty".to_string());
    }

    serde_json::from_slice(record).map_err(|error| format!("invalid DSH JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{decode_jsonl_record, encode_jsonl};
    use serde_json::json;

    #[test]
    fn encode_jsonl_appends_exactly_one_lf() {
        let encoded = encode_jsonl(&json!({ "jsonrpc": "2.0" })).expect("encode frame");
        assert_eq!(encoded, b"{\"jsonrpc\":\"2.0\"}\n");
    }

    #[test]
    fn decode_accepts_one_trailing_cr_but_never_splits_on_it() {
        let value = decode_jsonl_record(b"{\"a\":1}\r").expect("decode CR-terminated record");
        assert_eq!(value, json!({ "a": 1 }));
    }

    #[test]
    fn decode_rejects_embedded_lf_and_empty_records() {
        assert!(decode_jsonl_record(b"{\"a\":\n1}").is_err());
        assert!(decode_jsonl_record(b"").is_err());
    }

    #[test]
    fn decode_rejects_malformed_json() {
        assert!(decode_jsonl_record(b"not-json").is_err());
    }
}
