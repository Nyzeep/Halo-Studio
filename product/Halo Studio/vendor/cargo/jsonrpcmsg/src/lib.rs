//! # jsonrpcmsg
//!
//! A Rust library to build and parse JSON RPC messages
//!
//! ## Features
//!
//! - Supports JSON RPC v1.0, v1.1 and version 2.0 formats
//! - Serialize and deserialize JSON-RPC requests, responses, and batches
//! - Comprehensive error handling
//!
//! ## Usage
//!
//! ```rust
//! use jsonrpcmsg::{Request, Response, Error, Version};
//!
//! // Create a new JSON-RPC 2.0 request
//! let request = Request::new_v2(
//!     "subtract".to_string(),
//!     None,
//!     Some(jsonrpcmsg::request::Id::Number(1))
//! );
//!
//! // Serialize to JSON string
//! let json_string = jsonrpcmsg::serialize::to_request_string(&request).unwrap();
//!
//! // Deserialize from JSON string
//! let parsed_request = jsonrpcmsg::deserialize::from_request_string(&json_string).unwrap();
//! ```

// Re-export modules
pub mod version;
pub mod error;
pub mod request;
pub mod response;
pub mod batch;
pub mod serialize;
pub mod deserialize;

// Re-export key types
pub use version::Version;
pub use error::Error;
pub use request::{Request, Params, Id};
pub use response::Response;
pub use batch::{Message, Batch};

// Re-export convenience functions
pub use serialize::{
    to_request_string,
    to_response_string,
    to_batch_string,
    to_request_value,
    to_response_value,
    to_batch_value,
    SerializationError,
};

pub use deserialize::{
    from_request_string,
    from_response_string,
    from_batch_string,
    from_request_value,
    from_response_value,
    from_batch_value,
    DeserializationError,
};