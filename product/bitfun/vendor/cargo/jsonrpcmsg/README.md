# jsonrpcmsg

A Rust library to serialize (encode) and deserialize (parse) JSON-RPC messages.

## Features

- Supports JSON RPC v1.0, v1.1 and version 2.0 formats
- Serialize (encoder) and deserialize (parser) JSON-RPC requests, responses, and batches
- Comprehensive error handling

## Use cases

- **Important** - It does not implement the client or server.
- JSON-RPC validator
- JSON-RPC server or client development

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
jsonrpcmsg = "0.1.0"
```

Or run:

```bash
cargo add jsonrpcmsg
```

## Usage

The examples below are for JSON-RPC 2.0.

*Note* - *For version 1 or 1.1: check in source file documentation*

### Working with JSON-RPC Requests

#### Requests Serialization

Requests serialization is performed on the client side, sending the requests.

```rust
use jsonrpcmsg::{Request, Params, Id};

// Create a new JSON-RPC 2.0 request with parameters
let params = Some(Params::Array(vec![serde_json::Value::from(42), serde_json::Value::from(23)]));
let request = Request::new(
    "subtract".to_string(),
    params,
    Some(Id::Number(1))
);

// Serialize to JSON string
let json_string = jsonrpcmsg::serialize::to_request_string(&request).unwrap();
println!("Serialized request: {}", json_string);
```

Here's the corresponding valid JSON-RPC 2.0 request example to be sent:

```json
{"jsonrpc": "2.0", "method": "subtract", "params": [42, 23], "id": 1}
```

#### Requests Deserialization (parsing)

Requests deserialization is performed on the server side, receiving the requests.

This library will deserialize a JSON string message into a Request object that can be processed by your application.

```rust
use jsonrpcmsg::{Request};
// Deserialize from JSON string
let parsed_request = jsonrpcmsg::deserialize::from_request_string(&json_string).unwrap();
println!("Deserialized request method: {}", parsed_request.method);
```

### Working with JSON-RPC Responses

#### Responses Serialization

Response serialization is performed on the server side, sending the responses.

```rust
use jsonrpcmsg::{Response, Error, Id};
use serde_json::json;

// Create a successful response
let result = json!(19);
let response = Response::success(result, Some(Id::Number(1)));

// Serialize to JSON string
let response_json = jsonrpcmsg::serialize::to_response_string(&response).unwrap();
println!("Serialized response: {}", response_json);
```

Here's the corresponding valid JSON-RPC 2.0 response example to be sent:

```json
{"jsonrpc": "2.0", "result": 19, "id": 1}
```

#### Response Deserialization (parsing)

Response deserialization is performed on the client side, receiving the responses.

This library will deserialize a JSON string message into a Response object that can be processed by your application.

```rust
use jsonrpcmsg::{Response};
// Deserialize from JSON string
let parsed_response = jsonrpcmsg::deserialize::from_response_string(&json_string).unwrap();
println!("Deserialized response result: {}", parsed_request.result);
```

### Batch Processing

```rust
use jsonrpcmsg::{Batch, Message, Request, Response, Params, Id};

// Create a batch with multiple requests
let mut batch = Batch::new();
let params1 = Some(Params::Array(vec![serde_json::Value::from(1), serde_json::Value::from(2), serde_json::Value::from(3)]));
let params2 = Some(Params::Array(vec![serde_json::Value::from(42), serde_json::Value::from(23)]));

batch.push(Message::Request(Request::new(
    "sum".to_string(),
    params1,
    Some(Id::String("1".to_string()))
)));
batch.push(Message::Request(Request::new(
    "subtract".to_string(),
    params2,
    Some(Id::String("2".to_string()))
)));

// Serialize batch to JSON
let batch_json = jsonrpcmsg::serialize::to_batch_string(&batch).unwrap();
println!("Serialized batch: {}", batch_json);

// Deserialize batch from JSON
let parsed_batch = jsonrpcmsg::deserialize::from_batch_string(&batch_json).unwrap();
println!("Deserialized batch length: {}", parsed_batch.len());
```

## Supported JSON-RPC Versions

This library supports all major JSON-RPC specifications:

- **JSON-RPC 1.0**: The original specification with basic request/response format
- **JSON-RPC 1.1**: Intermediate specification with named parameters and batch support
- **JSON-RPC 2.0**: Current widely adopted specification with improved error handling and batch processing

## Dependencies

- [serde](https://crates.io/crates/serde), [serde_json](https://crates.io/crates/serde_json) for JSON serialization

## Testing

```bash
cargo test
```

This library is tested against a [set of valid](/examples/valid/) and [invalid messages](/examples/invalid/) contained in the official JSON-RPC [2.0](https://www.jsonrpc.org/specification) and [1.0](https://www.jsonrpc.org/specification_v1) specifications and on the [Wikipedia page dedicated to JSON-RPC](https://en.wikipedia.org/wiki/JSON-RPC).

## License

[MIT](mit.txt) or [Apache2](license.txt)

## Author

[David HEURTEVENT - frua.fr](https://github.com/fruafr)
