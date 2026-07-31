use jsonrpcmsg::error::Error;
use serde_json::json;

#[test]
fn test_error_creation() {
    let error = Error::new(-32601, "Method not found".to_string());
    assert_eq!(error.code, -32601);
    assert_eq!(error.message, "Method not found");
    assert_eq!(error.data, None);
}

#[test]
fn test_error_with_data() {
    let data = json!({"details": "function not found"});
    let error = Error::with_data(-32601, "Method not found".to_string(), data.clone());
    assert_eq!(error.code, -32601);
    assert_eq!(error.message, "Method not found");
    assert_eq!(error.data, Some(data));
}

#[test]
fn test_predefined_errors() {
    let error = Error::parse_error();
    assert_eq!(error.code, -32700);
    assert_eq!(error.message, "Parse error");

    let error = Error::invalid_request();
    assert_eq!(error.code, -32600);
    assert_eq!(error.message, "Invalid Request");

    let error = Error::method_not_found();
    assert_eq!(error.code, -32601);
    assert_eq!(error.message, "Method not found");

    let error = Error::invalid_params();
    assert_eq!(error.code, -32602);
    assert_eq!(error.message, "Invalid params");

    let error = Error::internal_error();
    assert_eq!(error.code, -32603);
    assert_eq!(error.message, "Internal error");

    let error = Error::server_error(-32000, "Custom server error".to_string());
    assert_eq!(error.code, -32000);
    assert_eq!(error.message, "Custom server error");

    // Test invalid server error code
    let error = Error::server_error(-31000, "Invalid code".to_string());
    assert_eq!(error.code, -32603); // Should fallback to internal error
    assert_eq!(error.message, "Internal error");
}