use jsonrpcmsg::{
    Request, Response, Batch, Message, Params, Id, Error,
    to_request_string, to_response_string, to_batch_string,
    from_request_string, from_response_string, from_batch_string
};
use jsonrpcmsg::deserialize::DeserializationError;
use serde_json::{json, Map};

#[test]
fn test_request_serialization_deserialization() {
    // Create a request with array params
    let array_params = vec![json!(42), json!(23)];
    let request = Request::new_v2(
        "subtract".to_string(),
        Some(Params::Array(array_params)),
        Some(Id::Number(1))
    );
    
    // Serialize to string
    let json_string = to_request_string(&request).unwrap();
    
    // Deserialize from string
    let parsed_request = from_request_string(&json_string).unwrap();
    
    // Check that they're equal
    assert_eq!(request, parsed_request);
    
    // Create a request with object params
    let mut map = Map::new();
    map.insert("minuend".to_string(), json!(42));
    map.insert("subtrahend".to_string(), json!(23));
    let request2 = Request::new_v2(
        "subtract".to_string(),
        Some(Params::Object(map)),
        Some(Id::String("abc".to_string()))
    );
    
    // Serialize to string
    let json_string2 = to_request_string(&request2).unwrap();
    
    // Deserialize from string
    let parsed_request2 = from_request_string(&json_string2).unwrap();
    
    // Check that they're equal
    assert_eq!(request2, parsed_request2);
}

#[test]
fn test_response_serialization_deserialization() {
    // Create a success response
    let result = json!(19);
    let response = Response::success_v2(result, Some(Id::Number(1)));
    
    // Serialize to string
    let json_string = to_response_string(&response).unwrap();
    
    // Deserialize from string
    let parsed_response = from_response_string(&json_string).unwrap();
    
    // Check that they're equal
    assert_eq!(response, parsed_response);
    
    // Create an error response
    let error = Error::method_not_found();
    let response2 = Response::error_v2(error, Some(Id::Number(2)));
    
    // Serialize to string
    let json_string2 = to_response_string(&response2).unwrap();
    
    // Deserialize from string
    let parsed_response2 = from_response_string(&json_string2).unwrap();
    
    // Check that they're equal
    assert_eq!(response2, parsed_response2);
}

#[test]
fn test_batch_serialization_deserialization() {
    let mut batch = Batch::new();
    
    // Add a request to the batch
    let request = Request::new_v2(
        "subtract".to_string(),
        Some(Params::Array(vec![json!(42), json!(23)])),
        Some(Id::Number(1))
    );
    batch.add_request(request);
    
    // Add a response to the batch
    let result = json!(19);
    let response = Response::success_v2(result, Some(Id::Number(1)));
    batch.add_response(response);
    
    // Serialize to string
    let json_string = to_batch_string(&batch).unwrap();
    
    // Deserialize from string
    let parsed_batch = from_batch_string(&json_string).unwrap();
    
    // Check that they're equal
    assert_eq!(batch.len(), parsed_batch.len());
    
    // Check first message (request)
    match (&batch[0], &parsed_batch[0]) {
        (Message::Request(original), Message::Request(parsed)) => {
            assert_eq!(original, parsed);
        }
        _ => panic!("Expected Request messages"),
    }
    
    // Check second message (response)
    match (&batch[1], &parsed_batch[1]) {
        (Message::Response(original), Message::Response(parsed)) => {
            assert_eq!(original, parsed);
        }
        _ => panic!("Expected Response messages"),
    }
}

#[test]
fn test_notification_serialization() {
    // Create a notification (no id)
    let notification = Request::notification_v2(
        "update".to_string(),
        Some(Params::Array(vec![json!(1), json!(2), json!(3)]))
    );
    
    // Serialize to string
    let json_string = to_request_string(&notification).unwrap();
    
    // Deserialize from string
    let parsed_notification = from_request_string(&json_string).unwrap();
    
    // Check that they're equal
    assert_eq!(notification, parsed_notification);
    
    // Verify it's a notification
    assert!(parsed_notification.is_notification());
}

#[test]
fn test_serialization_error() {
    // Try to serialize a request with invalid data
    // This test is more to ensure our error types are working
    // In practice, serde_json::to_string rarely fails for valid structs
}

#[test]
fn test_deserialization_error() {
    // Try to deserialize invalid JSON
    let invalid_json = "{ invalid json }";
    let result = from_request_string(invalid_json);
    
    // Should be an error
    assert!(result.is_err());
    
    // Check error type
    match result {
        Err(DeserializationError::JsonError(_)) => {},
        _ => panic!("Expected JsonError"),
    }
}