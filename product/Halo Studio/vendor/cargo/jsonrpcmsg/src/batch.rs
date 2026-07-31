//! JSON-RPC batch types
use serde::{Deserialize, Serialize};
use crate::request::Request;
use crate::response::Response;

/// A message in a batch, which can be either a request or a response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    /// A request message
    Request(Request),
    /// A response message
    Response(Response),
}

/// A batch of JSON-RPC messages
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Batch(Vec<Message>);

impl Batch {
    /// Create a new empty batch
    pub fn new() -> Self {
        Batch(Vec::new())
    }
    
    /// Create a new batch with the given capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Batch(Vec::with_capacity(capacity))
    }
    
    /// Add a request to the batch
    pub fn add_request(&mut self, request: Request) {
        self.0.push(Message::Request(request));
    }
    
    /// Add a response to the batch
    pub fn add_response(&mut self, response: Response) {
        self.0.push(Message::Response(response));
    }
    
    /// Check if the batch is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    
    /// Get the number of messages in the batch
    pub fn len(&self) -> usize {
        self.0.len()
    }
    
    /// Get a reference to the underlying vector
    pub fn as_vec(&self) -> &Vec<Message> {
        &self.0
    }
    
    /// Get a mutable reference to the underlying vector
    pub fn as_vec_mut(&mut self) -> &mut Vec<Message> {
        &mut self.0
    }
}

impl Message {
    /// Check if this message is a request
    pub fn is_request(&self) -> bool {
        matches!(self, Message::Request(_))
    }
    
    /// Check if this message is a response
    pub fn is_response(&self) -> bool {
        matches!(self, Message::Response(_))
    }
    
    /// Get the message as a request, if it is one
    pub fn as_request(&self) -> Option<&Request> {
        match self {
            Message::Request(req) => Some(req),
            Message::Response(_) => None,
        }
    }
    
    /// Get the message as a response, if it is one
    pub fn as_response(&self) -> Option<&Response> {
        match self {
            Message::Request(_) => None,
            Message::Response(res) => Some(res),
        }
    }
}

// Implement Deref and DerefMut to allow using Batch like a Vec<Message>
impl std::ops::Deref for Batch {
    type Target = Vec<Message>;
    
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Batch {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// Implement IntoIterator to allow iterating over Batch
impl IntoIterator for Batch {
    type Item = Message;
    type IntoIter = std::vec::IntoIter<Message>;
    
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Batch {
    type Item = &'a Message;
    type IntoIter = std::slice::Iter<'a, Message>;
    
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut Batch {
    type Item = &'a mut Message;
    type IntoIter = std::slice::IterMut<'a, Message>;
    
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

// Implement From<Vec<Message>> for Batch
impl From<Vec<Message>> for Batch {
    fn from(vec: Vec<Message>) -> Self {
        Batch(vec)
    }
}