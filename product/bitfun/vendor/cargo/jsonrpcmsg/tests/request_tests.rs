use jsonrpcmsg::request::{Request, Params, Id};
use jsonrpcmsg::version::Version;
use serde_json::{json, Map};

#[test]
fn test_request_creation() {
    let request = Request::new(
        "subtract".to_string(),
        None,
        Some(Id::Number(1))
    );
    
    assert_eq!(request.jsonrpc, Some("2.0".to_string()));
    assert_eq!(request.version, None);
    assert_eq!(request.method, "subtract");
    assert_eq!(request.params, None);
    assert_eq!(request.id, Some(Id::Number(1)));
}

#[test]
fn test_request_v2_creation() {
    let request = Request::new_v2(
        "subtract".to_string(),
        None,
        Some(Id::Number(1))
    );
    
    assert_eq!(request.jsonrpc, Some("2.0".to_string()));
    assert_eq!(request.version, None);
    assert_eq!(request.method, "subtract");
    assert_eq!(request.params, None);
    assert_eq!(request.id, Some(Id::Number(1)));
}

#[test]
fn test_request_v1_1_creation() {
    let request = Request::new_v1_1(
        "subtract".to_string(),
        None,
        Some(Id::Number(1))
    );
    
    assert_eq!(request.jsonrpc, None);
    assert_eq!(request.version, Some("1.1".to_string()));
    assert_eq!(request.method, "subtract");
    assert_eq!(request.params, None);
    assert_eq!(request.id, Some(Id::Number(1)));
}

#[test]
fn test_request_v1_creation() {
    let request = Request::new_v1(
        "subtract".to_string(),
        None,
        Some(Id::Number(1))
    );
    
    assert_eq!(request.jsonrpc, None);
    assert_eq!(request.version, None);
    assert_eq!(request.method, "subtract");
    assert_eq!(request.params, None);
    assert_eq!(request.id, Some(Id::Number(1)));
}

#[test]
fn test_notification_creation() {
    let request = Request::notification("update".to_string(), None);
    
    assert_eq!(request.jsonrpc, Some("2.0".to_string()));
    assert_eq!(request.version, None);
    assert_eq!(request.method, "update");
    assert_eq!(request.params, None);
    assert_eq!(request.id, None);
}

#[test]
fn test_notification_v2_creation() {
    let request = Request::notification_v2("update".to_string(), None);
    
    assert_eq!(request.jsonrpc, Some("2.0".to_string()));
    assert_eq!(request.version, None);
    assert_eq!(request.method, "update");
    assert_eq!(request.params, None);
    assert_eq!(request.id, None);
}

#[test]
fn test_is_notification() {
    let request = Request::new_v2(
        "subtract".to_string(),
        None,
        Some(Id::Number(1))
    );
    assert!(!request.is_notification());
    
    let notification = Request::notification_v2("update".to_string(), None);
    assert!(notification.is_notification());
}

#[test]
fn test_request_version() {
    let request_v2 = Request::new_v2(
        "subtract".to_string(),
        None,
        Some(Id::Number(1))
    );
    assert_eq!(request_v2.version(), Some(Version::V2_0));
    
    let request_v1_1 = Request::new_v1_1(
        "subtract".to_string(),
        None,
        Some(Id::Number(1))
    );
    assert_eq!(request_v1_1.version(), Some(Version::V1_1));
    
    let request_v1 = Request::new_v1(
        "subtract".to_string(),
        None,
        Some(Id::Number(1))
    );
    assert_eq!(request_v1.version(), Some(Version::V1_0));
}

#[test]
fn test_params_creation() {
    // Test array params
    let array_params = Params::Array(vec![json!(42), json!(23)]);
    match array_params {
        Params::Array(arr) => {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0], json!(42));
            assert_eq!(arr[1], json!(23));
        }
        _ => panic!("Expected Array params"),
    }
    
    // Test object params
    let mut map = Map::new();
    map.insert("minuend".to_string(), json!(42));
    map.insert("subtrahend".to_string(), json!(23));
    let object_params = Params::Object(map);
    match object_params {
        Params::Object(obj) => {
            assert_eq!(obj.len(), 2);
            assert_eq!(obj["minuend"], json!(42));
            assert_eq!(obj["subtrahend"], json!(23));
        }
        _ => panic!("Expected Object params"),
    }
}

#[test]
fn test_id_creation() {
    let string_id = Id::String("abc".to_string());
    match string_id {
        Id::String(s) => assert_eq!(s, "abc"),
        _ => panic!("Expected String ID"),
    }
    
    let number_id = Id::Number(123);
    match number_id {
        Id::Number(n) => assert_eq!(n, 123),
        _ => panic!("Expected Number ID"),
    }
    
    let null_id = Id::Null;
    match null_id {
        Id::Null => {},
        _ => panic!("Expected Null ID"),
    }
}