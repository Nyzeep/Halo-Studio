use jsonrpcmsg::response::Response;
use jsonrpcmsg::request::Id;
use jsonrpcmsg::error::Error;
use serde_json::json;

#[test]
fn test_response_success() {
    let result = json!(19);
    let response = Response::success_v2(result.clone(), Some(Id::Number(1)));
    
    assert_eq!(response.jsonrpc, Some("2.0".to_string()));
    assert_eq!(response.version, None);
    assert_eq!(response.result, Some(result));
    assert_eq!(response.error, None);
    assert_eq!(response.id, Some(Id::Number(1)));
}

#[test]
fn test_response_error() {
    let error = Error::method_not_found();
    let response = Response::error(error.clone(), Some(Id::Number(1)));
    
    assert_eq!(response.jsonrpc, Some("2.0".to_string()));
    assert_eq!(response.version, None);
    assert_eq!(response.result, None);
    assert_eq!(response.error, Some(error));
    assert_eq!(response.id, Some(Id::Number(1)));
}

#[test]
fn test_response_success_v2() {
    let result = json!(19);
    let response = Response::success_v2(result.clone(), Some(Id::Number(1)));
    
    assert_eq!(response.jsonrpc, Some("2.0".to_string()));
    assert_eq!(response.version, None);
    assert_eq!(response.result, Some(result));
    assert_eq!(response.error, None);
    assert_eq!(response.id, Some(Id::Number(1)));
}

#[test]
fn test_response_error_v2() {
    let error = Error::method_not_found();
    let response = Response::error_v2(error.clone(), Some(Id::Number(1)));
    
    assert_eq!(response.jsonrpc, Some("2.0".to_string()));
    assert_eq!(response.version, None);
    assert_eq!(response.result, None);
    assert_eq!(response.error, Some(error));
    assert_eq!(response.id, Some(Id::Number(1)));
}

#[test]
fn test_response_success_v1_1() {
    let result = json!(19);
    let response = Response::success_v1_1(result.clone(), Some(Id::Number(1)));
    
    assert_eq!(response.jsonrpc, None);
    assert_eq!(response.version, Some("1.1".to_string()));
    assert_eq!(response.result, Some(result));
    assert_eq!(response.error, None);
    assert_eq!(response.id, Some(Id::Number(1)));
}

#[test]
fn test_response_error_v1_1() {
    let error = Error::method_not_found();
    let response = Response::error_v1_1(error.clone(), Some(Id::Number(1)));
    
    assert_eq!(response.jsonrpc, None);
    assert_eq!(response.version, Some("1.1".to_string()));
    assert_eq!(response.result, None);
    assert_eq!(response.error, Some(error));
    assert_eq!(response.id, Some(Id::Number(1)));
}

#[test]
fn test_response_success_v1() {
    let result = json!(19);
    let response = Response::success_v1(result.clone(), Some(Id::Number(1)));
    
    assert_eq!(response.jsonrpc, None);
    assert_eq!(response.version, None);
    assert_eq!(response.result, Some(result));
    assert_eq!(response.error, None);
    assert_eq!(response.id, Some(Id::Number(1)));
}

#[test]
fn test_response_error_v1() {
    let error = Error::method_not_found();
    let response = Response::error_v1(error.clone(), Some(Id::Number(1)));
    
    assert_eq!(response.jsonrpc, None);
    assert_eq!(response.version, None);
    assert_eq!(response.result, None);
    assert_eq!(response.error, Some(error));
    assert_eq!(response.id, Some(Id::Number(1)));
}

#[test]
fn test_response_is_success() {
    let result = json!(19);
    let success_response = Response::success_v2(result, Some(Id::Number(1)));
    assert!(success_response.is_success());
    assert!(!success_response.is_error());
}

#[test]
fn test_response_is_error() {
    let error = Error::method_not_found();
    let error_response = Response::error_v2(error, Some(Id::Number(1)));
    assert!(error_response.is_error());
    assert!(!error_response.is_success());
}