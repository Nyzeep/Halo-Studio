# JSON-RPC Message Specifications

This document describes the message formats for JSON-RPC versions 1.0, 1.1, and 2.0.

## JSON-RPC 1.0

JSON-RPC 1.0 is the original specification for remote procedure calls using JSON.

### Request Object

A request object has the following properties:

- `method` (String): The name of the method to be invoked.
- `params` (Array): An array of parameters to be passed to the method.
- `id` (String or Number): The identifier for the request, used to match responses.

Example:
```json
{
  "method": "subtract",
  "params": [42, 23],
  "id": 1
}
```

### Notification

A notification is a request without an `id` field. The server does not need to respond to notifications.

Example:
```json
{
  "method": "update",
  "params": [1, 2, 3, 4, 5]
}
```

### Response Object

A response object has the following properties:

- `result`: The result of the method invocation (present only if successful).
- `error`: An error object (present only if an error occurred).
- `id`: The identifier matching the request.

Either `result` or `error` must be present, but not both.

Example (success):
```json
{
  "result": 19,
  "error": null,
  "id": 1
}
```

Example (error):
```json
{
  "result": null,
  "error": {
    "code": -32601,
    "message": "Method not found"
  },
  "id": 1
}
```

### Error Object

The error object has the following properties:

- `code` (Number): A number indicating the error type.
- `message` (String): A short description of the error.
- `data` (Optional): Additional information about the error.

## JSON-RPC 1.1

JSON-RPC 1.1 was an intermediate specification that introduced some improvements but was never widely adopted.

### Key Differences from 1.0

1. **Version Field**: Added a `version` field to distinguish from 1.0.
2. **Named Parameters**: Support for named parameters in addition to positional parameters.
3. **Batch Requests**: Support for batch requests (multiple requests in a single message).

### Request Object

- `version` (String): Must be "1.1".
- `method` (String): The name of the method to be invoked.
- `params` (Array or Object): Parameters to be passed to the method.
- `id` (String or Number): The identifier for the request.

Example:
```json
{
  "version": "1.1",
  "method": "subtract",
  "params": [42, 23],
  "id": 1
}
```

### Named Parameters

Parameters can be specified as an object with named keys:

```json
{
  "version": "1.1",
  "method": "subtract",
  "params": {
    "minuend": 42,
    "subtrahend": 23
  },
  "id": 1
}
```

### Batch Requests

Multiple requests can be sent in an array:

```json
[
  {
    "version": "1.1",
    "method": "add",
    "params": [1, 2],
    "id": 1
  },
  {
    "version": "1.1",
    "method": "subtract",
    "params": [3, 1],
    "id": 2
  }
]
```

### Response Object

Similar to 1.0 but with version field:

- `version` (String): Must be "1.1".
- `result`: The result of the method invocation.
- `error`: An error object if an error occurred.
- `id`: The identifier matching the request.

## JSON-RPC 2.0

JSON-RPC 2.0 is the current widely adopted specification.

### Key Differences from 1.0 and 1.1

1. **Version Field**: Added a `jsonrpc` field with value "2.0".
2. **Single Error Format**: Standardized error format.
3. **Removed 1.1 Features**: Removed some features from 1.1.
4. **Improved Batch Handling**: Better handling of batch requests and notifications.

### Request Object

A request object has the following properties:

- `jsonrpc` (String): Must be "2.0".
- `method` (String): The name of the method to be invoked.
- `params` (Optional): Parameters to be passed to the method (Array or Object).
- `id` (Optional): The identifier for the request.

Example:
```json
{
  "jsonrpc": "2.0",
  "method": "subtract",
  "params": [42, 23],
  "id": 1
}
```

### Notification

A notification is a request without an `id` field:

```json
{
  "jsonrpc": "2.0",
  "method": "update",
  "params": [1, 2, 3, 4, 5]
}
```

### Parameter Structures

Parameters can be either positional (array) or named (object):

Positional:
```json
{
  "jsonrpc": "2.0",
  "method": "subtract",
  "params": [42, 23],
  "id": 1
}
```

Named:
```json
{
  "jsonrpc": "2.0",
  "method": "subtract",
  "params": {
    "subtrahend": 23,
    "minuend": 42
  },
  "id": 1
}
```

### Response Object

- `jsonrpc` (String): Must be "2.0".
- `result`: The result of the method invocation (required on success).
- `error`: An error object (required on error).
- `id`: The identifier matching the request (null for notifications).

Either `result` or `error` must be present, but not both.

Example (success):
```json
{
  "jsonrpc": "2.0",
  "result": 19,
  "id": 1
}
```

Example (error):
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32601,
    "message": "Method not found"
  },
  "id": 1
}
```

### Batch Requests

Multiple requests, responses, and notifications can be sent in an array:

```json
[
  {
    "jsonrpc": "2.0",
    "method": "add",
    "params": [1, 2],
    "id": 1
  },
  {
    "jsonrpc": "2.0",
    "method": "subtract",
    "params": [3, 1],
    "id": 2
  }
]
```

For batch requests, the server should respond with an array of responses:

```json
[
  {
    "jsonrpc": "2.0",
    "result": 3,
    "id": 1
  },
  {
    "jsonrpc": "2.0",
    "result": 2,
    "id": 2
  }
]
```

### Error Object

The error object has the following properties:

- `code` (Number): A number indicating the error type.
- `message` (String): A short description of the error.
- `data` (Optional): Additional information about the error.

Predefined error codes:
- -32700: Parse error
- -32600: Invalid Request
- -32601: Method not found
- -32602: Invalid params
- -32603: Internal error
- -32000 to -32099: Server error

## Summary of Differences

| Feature | JSON-RPC 1.0 | JSON-RPC 1.1 | JSON-RPC 2.0 |
|---------|--------------|--------------|--------------|
| Version Field | None | `version`: "1.1" | `jsonrpc`: "2.0" |
| Named Parameters | No | Yes | Yes |
| Batch Requests | No | Yes | Yes (improved) |
| Notifications | No `id` field | No `id` field | No `id` field |
| Error Format | Basic | Basic | Standardized |
| Adoption | Limited | Limited | Widespread |