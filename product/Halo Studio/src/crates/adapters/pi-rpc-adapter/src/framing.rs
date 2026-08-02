use serde::Serialize;
use serde_json::Value;

pub(crate) fn encode_jsonl<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let mut encoded = serde_json::to_vec(value)?;
    encoded.push(b'\n');
    Ok(encoded)
}

pub(crate) fn decode_jsonl_record(record: &[u8]) -> Result<Value, String> {
    if record.contains(&b'\n') {
        return Err("Pi RPC records must contain one LF delimiter".to_string());
    }

    // Pi emits LF records and accepts a trailing CR for clients that use CRLF.
    // Strip only that framing byte; a CR anywhere else remains invalid JSON.
    let record = record.strip_suffix(b"\r").unwrap_or(record);
    serde_json::from_slice(record).map_err(|error| format!("invalid Pi RPC JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{decode_jsonl_record, encode_jsonl};

    #[test]
    fn encodes_one_json_record_with_an_lf_delimiter() {
        let encoded = encode_jsonl(&json!({ "type": "prompt", "message": "hello" }))
            .expect("JSON encoding succeeds");

        assert_eq!(encoded.last(), Some(&b'\n'));
        assert_eq!(encoded.iter().filter(|byte| **byte == b'\n').count(), 1);
        assert!(!encoded[..encoded.len() - 1].contains(&b'\r'));
    }

    #[test]
    fn accepts_an_optional_cr_before_the_lf_delimiter() {
        let value =
            decode_jsonl_record(br#"{"type":"agent_settled"}"#).expect("JSON record decodes");
        assert_eq!(value["type"], "agent_settled");

        let mut crlf_record = br#"{"type":"agent_settled"}"#.to_vec();
        crlf_record.push(b'\r');
        let value = decode_jsonl_record(&crlf_record).expect("CRLF record decodes");
        assert_eq!(value["type"], "agent_settled");
    }

    #[test]
    fn rejects_an_embedded_raw_lf() {
        assert!(decode_jsonl_record(b"{\"type\":\"agent_settled\"}\n").is_err());
    }
}
