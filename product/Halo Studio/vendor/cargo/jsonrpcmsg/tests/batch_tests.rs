use jsonrpcmsg::batch::{Batch, Message};
use jsonrpcmsg::request::{Request, Id};
use jsonrpcmsg::response::Response;
use serde_json::json;

#[test]
fn test_batch_creation() {
    let batch = Batch::new();
    assert!(batch.is_empty());
    assert_eq!(batch.len(), 0);
}

#[test]
fn test_batch_with_capacity() {
    let batch = Batch::with_capacity(10);
    assert!(batch.is_empty());
    assert_eq!(batch.len(), 0);
}

#[test]
fn test_batch_add_request() {
    let mut batch = Batch::new();
    
    let request = Request::new_v2(
        "subtract".to_string(),
        None,
        Some(Id::Number(1))
    );
    
    batch.add_request(request.clone());
    
    assert!(!batch.is_empty());
    assert_eq!(batch.len(), 1);
    
    match &batch[0] {
        Message::Request(req) => {
            assert_eq!(req.method, "subtract");
            assert_eq!(req.id, Some(Id::Number(1)));
        }
        _ => panic!("Expected Request message"),
    }
}

#[test]
fn test_batch_add_response() {
    let mut batch = Batch::new();
    
    let result = json!(19);
    let response = Response::success_v2(result, Some(Id::Number(1)));
    
    batch.add_response(response.clone());
    
    assert!(!batch.is_empty());
    assert_eq!(batch.len(), 1);
    
    match &batch[0] {
        Message::Response(res) => {
            assert!(res.is_success());
            assert_eq!(res.id, Some(Id::Number(1)));
        }
        _ => panic!("Expected Response message"),
    }
}

#[test]
fn test_message_type_checks() {
    let request = Request::new_v2(
        "subtract".to_string(),
        None,
        Some(Id::Number(1))
    );
    let message_request = Message::Request(request);
    
    assert!(message_request.is_request());
    assert!(!message_request.is_response());
    assert!(message_request.as_request().is_some());
    assert!(message_request.as_response().is_none());
    
    let result = json!(19);
    let response = Response::success_v2(result, Some(Id::Number(1)));
    let message_response = Message::Response(response);
    
    assert!(!message_response.is_request());
    assert!(message_response.is_response());
    assert!(message_response.as_request().is_none());
    assert!(message_response.as_response().is_some());
}

#[test]
fn test_batch_iteration() {
    let mut batch = Batch::new();
    
    let request = Request::new_v2(
        "subtract".to_string(),
        None,
        Some(Id::Number(1))
    );
    batch.add_request(request);
    
    let result = json!(19);
    let response = Response::success_v2(result, Some(Id::Number(2)));
    batch.add_response(response);
    
    assert_eq!(batch.len(), 2);
    
    // Test iteration
    let mut count = 0;
    for message in &batch {
        match (count, message) {
            (0, Message::Request(_)) => {},
            (1, Message::Response(_)) => {},
            _ => panic!("Unexpected message type or order"),
        }
        count += 1;
    }
    
    assert_eq!(count, 2);
}