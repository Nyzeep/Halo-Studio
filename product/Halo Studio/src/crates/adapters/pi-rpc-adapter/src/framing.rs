//! Strict LF-delimited framing for Pi RPC.

use serde::Serialize;
use serde_json::Value;

pub(crate) fn encode_jsonl<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let mut encoded = serde_json::to_vec(value)?;
    encoded.push(b'\n');
    Ok(encoded)
}

/// Decode exactly one record after its LF delimiter has been removed.
///
/// Pi's wire contract is LF-only. We accept one trailing CR for input
/// interoperability, but never split on CR, U+2028, or U+2029.
pub(crate) fn decode_jsonl_record(record: &[u8]) -> Result<Value, String> {
    if record.contains(&b'\n') {
        return Err("Pi RPC record contains an embedded LF".to_string());
    }

    let record = record.strip_suffix(b"\r").unwrap_or(record);
    if record.is_empty() {
        return Err("Pi RPC record is empty".to_string());
    }

    serde_json::from_slice(record).map_err(|error| format!("invalid Pi RPC JSON: {error}"))
}
