use jsonrpcmsg::{
    Request, Message, Params, Id,
    to_request_string, to_response_string,
    from_request_string, from_response_string, from_batch_string
};
use serde_json::json;
use std::fs;

#[test]
fn test_jsonrpc_2_0_spec_examples() {
    // Test example from JSON-RPC 2.0 spec: Request with positional parameters
    let request_json = r#"{"jsonrpc": "2.0", "method": "subtract", "params": [42, 23], "id": 1}"#;
    let parsed_request = from_request_string(request_json).unwrap();
    
    assert_eq!(parsed_request.jsonrpc, Some("2.0".to_string()));
    assert_eq!(parsed_request.method, "subtract");
    assert_eq!(parsed_request.id, Some(Id::Number(1)));
    
    match parsed_request.params {
        Some(Params::Array(params)) => {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0], json!(42));
            assert_eq!(params[1], json!(23));
        }
        _ => panic!("Expected array parameters"),
    }
    
    // Test example from JSON-RPC 2.0 spec: Request with named parameters
    let request_json2 = r#"{"jsonrpc": "2.0", "method": "subtract", "params": {"subtrahend": 23, "minuend": 42}, "id": 3}"#;
    let parsed_request2 = from_request_string(request_json2).unwrap();
    
    assert_eq!(parsed_request2.jsonrpc, Some("2.0".to_string()));
    assert_eq!(parsed_request2.method, "subtract");
    assert_eq!(parsed_request2.id, Some(Id::Number(3)));
    
    match parsed_request2.params {
        Some(Params::Object(params)) => {
            assert_eq!(params.len(), 2);
            assert_eq!(params["subtrahend"], json!(23));
            assert_eq!(params["minuend"], json!(42));
        }
        _ => panic!("Expected object parameters"),
    }
    
    // Test example from JSON-RPC 2.0 spec: Notification
    let notification_json = r#"{"jsonrpc": "2.0", "method": "update", "params": [1,2,3,4,5]}"#;
    let parsed_notification = from_request_string(notification_json).unwrap();
    
    assert_eq!(parsed_notification.jsonrpc, Some("2.0".to_string()));
    assert_eq!(parsed_notification.method, "update");
    assert_eq!(parsed_notification.id, None);
    assert!(parsed_notification.is_notification());
    
    match parsed_notification.params {
        Some(Params::Array(params)) => {
            assert_eq!(params.len(), 5);
            assert_eq!(params[0], json!(1));
            assert_eq!(params[4], json!(5));
        }
        _ => panic!("Expected array parameters"),
    }
    
    // Test example from JSON-RPC 2.0 spec: Success response
    let response_json = r#"{"jsonrpc": "2.0", "result": 19, "id": 1}"#;
    let parsed_response = from_response_string(response_json).unwrap();
    
    assert_eq!(parsed_response.jsonrpc, Some("2.0".to_string()));
    assert_eq!(parsed_response.result, Some(json!(19)));
    assert_eq!(parsed_response.error, None);
    assert_eq!(parsed_response.id, Some(Id::Number(1)));
    assert!(parsed_response.is_success());
    
    // Test example from JSON-RPC 2.0 spec: Error response
    let error_response_json = r#"{"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found"}, "id": "1"}"#;
    let parsed_error_response = from_response_string(error_response_json).unwrap();
    
    assert_eq!(parsed_error_response.jsonrpc, Some("2.0".to_string()));
    assert_eq!(parsed_error_response.result, None);
    assert_eq!(parsed_error_response.id, Some(Id::String("1".to_string())));
    assert!(parsed_error_response.is_error());
    
    match parsed_error_response.error {
        Some(error) => {
            assert_eq!(error.code, -32601);
            assert_eq!(error.message, "Method not found");
        }
        None => panic!("Expected error"),
    }
}

#[test]
fn test_jsonrpc_2_0_batch_example() {
    // Test example from JSON-RPC 2.0 spec: Batch request
    let batch_json = r#"[{"jsonrpc": "2.0", "method": "sum", "params": [1,2,4], "id": "1"}, {"jsonrpc": "2.0", "method": "notify_hello", "params": [7]}, {"jsonrpc": "2.0", "method": "subtract", "params": [42,23], "id": "2"}, {"jsonrpc": "2.0", "method": "get_data", "id": "9"}]"#;
    let parsed_batch = from_batch_string(batch_json).unwrap();
    
    assert_eq!(parsed_batch.len(), 4);
    
    // First request: sum
    match &parsed_batch[0] {
        Message::Request(req) => {
            assert_eq!(req.jsonrpc, Some("2.0".to_string()));
            assert_eq!(req.method, "sum");
            assert_eq!(req.id, Some(Id::String("1".to_string())));
        }
        _ => panic!("Expected Request"),
    }
    
    // Second request: notification
    match &parsed_batch[1] {
        Message::Request(req) => {
            assert_eq!(req.jsonrpc, Some("2.0".to_string()));
            assert_eq!(req.method, "notify_hello");
            assert_eq!(req.id, None);
            assert!(req.is_notification());
        }
        _ => panic!("Expected Request (notification)"),
    }
    
    // Third request: subtract
    match &parsed_batch[2] {
        Message::Request(req) => {
            assert_eq!(req.jsonrpc, Some("2.0".to_string()));
            assert_eq!(req.method, "subtract");
            assert_eq!(req.id, Some(Id::String("2".to_string())));
        }
        _ => panic!("Expected Request"),
    }
    
    // Fourth request: get_data
    match &parsed_batch[3] {
        Message::Request(req) => {
            assert_eq!(req.jsonrpc, Some("2.0".to_string()));
            assert_eq!(req.method, "get_data");
            assert_eq!(req.id, Some(Id::String("9".to_string())));
        }
        _ => panic!("Expected Request"),
    }
}

#[test]
fn test_roundtrip_with_spec_examples() {
    // Test that we can parse and then re-serialize JSON-RPC 2.0 examples correctly
    
    // Request with positional parameters
    let original_json = r#"{"jsonrpc": "2.0", "method": "subtract", "params": [42, 23], "id": 1}"#;
    let request = from_request_string(original_json).unwrap();
    let serialized_json = to_request_string(&request).unwrap();
    
    // Parse again to compare
    let reparsed_request = from_request_string(&serialized_json).unwrap();
    assert_eq!(request, reparsed_request);
    
    // Success response
    let original_response_json = r#"{"jsonrpc": "2.0", "result": 19, "id": 1}"#;
    let response = from_response_string(original_response_json).unwrap();
    let serialized_response_json = to_response_string(&response).unwrap();
    
    // Parse again to compare
    let reparsed_response = from_response_string(&serialized_response_json).unwrap();
    assert_eq!(response, reparsed_response);
}

#[test]
fn test_mixed_version_compatibility() {
    // Test that we can handle different JSON-RPC versions in the same application
    
    // JSON-RPC 2.0 request
    let request_v2 = Request::new_v2(
        "subtract".to_string(),
        Some(Params::Array(vec![json!(42), json!(23)])),
        Some(Id::Number(1))
    );
    
    // Serialize and deserialize
    let json_v2 = to_request_string(&request_v2).unwrap();
    let parsed_v2 = from_request_string(&json_v2).unwrap();
    
    // Verify versions
    assert_eq!(parsed_v2.jsonrpc, Some("2.0".to_string()));
}

/// Test all valid JSON-RPC examples from the examples/valid directory
#[test]
fn test_valid_examples() {
    // Test valid individual requests/responses
    let valid_files = [
        "examples/valid/request1.json",
        "examples/valid/request2.json",
        "examples/valid/request3.json",
        "examples/valid/request4.json",
        "examples/valid/request5.json",
        "examples/valid/request6.json",
        "examples/valid/request7.json",
        "examples/valid/request8.json",
        "examples/valid/response1.json",
        "examples/valid/response2.json",
        "examples/valid/response3.json",
        "examples/valid/response4.json",
        "examples/valid/response5.json",
        "examples/valid/output-error1.json",
        "examples/valid/output-error2.json",
        "examples/valid/output-error3.json",
        "examples/valid/output-error4.json",
        "examples/valid/jsonrpc1.1-request1.json",
        "examples/valid/jsonrpc1.1-response1.json",
        "examples/valid/jsonrpc1.request1.json",
        "examples/valid/jsonrpc1.response1.json",
    ];

    for file_path in &valid_files {
        let content = fs::read_to_string(file_path).expect(&format!("Failed to read {}", file_path));
        
        // Try parsing as request first
        if let Ok(_request) = from_request_string(&content) {
            // Successfully parsed as request
            continue;
        }
        
        // Try parsing as response
        if let Ok(_response) = from_response_string(&content) {
            // Successfully parsed as response
            continue;
        }
        
        // If we get here, it failed to parse as either
        panic!("Failed to parse valid example {}: {}", file_path, content);
    }
}

/// Test all valid batch examples from the examples/valid directory
#[test]
fn test_valid_batch_examples() {
    // Test valid batch requests
    let valid_batch_files = [
        "examples/valid/batch1.json",
        "examples/valid/batch2.json",
        "examples/valid/batch3.json",
        "examples/valid/output-batch-response1.json",
        "examples/valid/output-batch-error1.json",
        "examples/valid/output-batch-error2.json",
        "examples/valid/output-batch-error3.json",
        "examples/valid/output-batch-mixed.json",
    ];

    for file_path in &valid_batch_files {
        let content = fs::read_to_string(file_path).expect(&format!("Failed to read {}", file_path));
        
        // Try parsing as batch
        match from_batch_string(&content) {
            Ok(_batch) => {
                // Successfully parsed as batch
                continue;
            }
            Err(e) => {
                panic!("Failed to parse valid batch example {}: {}\nError: {:?}", file_path, content, e);
            }
        }
    }
}

/// Test all invalid JSON-RPC examples from the examples/invalid directory
#[test]
fn test_invalid_examples() {
    // Test invalid individual requests/responses
    let invalid_files = [
        "examples/invalid/invalid1.json",
        "examples/invalid/invalid2.json",
        "examples/invalid/invalid3.json",
        "examples/invalid/invalid4.json",
        "examples/invalid/invalid5.json",
    ];

    for file_path in &invalid_files {
        let content = fs::read_to_string(file_path).expect(&format!("Failed to read {}", file_path));
        
        // Try parsing as request
        if from_request_string(&content).is_ok() {
            panic!("Invalid example {} was parsed as valid request: {}", file_path, content);
        }
        
        // Try parsing as response
        if from_response_string(&content).is_ok() {
            panic!("Invalid example {} was parsed as valid response: {}", file_path, content);
        }
    }
}

/// Test all invalid batch examples from the examples/invalid directory
#[test]
fn test_invalid_batch_examples() {
    // Test invalid batch requests
    let invalid_batch_files = [
        "examples/invalid/invalid-batch1.json",
        "examples/invalid/invalid-batch2.json",
    ];

    for file_path in &invalid_batch_files {
        let content = fs::read_to_string(file_path).expect(&format!("Failed to read {}", file_path));
        
        // Try parsing as batch - should fail
        if from_batch_string(&content).is_ok() {
            panic!("Invalid batch example {} was parsed as valid batch: {}", file_path, content);
        }
    }
}