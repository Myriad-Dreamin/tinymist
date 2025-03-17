//! The request-response queue for the LSP server.
//!
//! This is stolen from the lsp-server crate for customization.

use core::fmt;
use std::collections::HashMap;

use crate::msg::RequestId;

#[cfg(feature = "lsp")]
use crate::lsp::{Request, Response};
#[cfg(feature = "lsp")]
use crate::msg::{ErrorCode, ResponseError};
#[cfg(feature = "lsp")]
use serde::Serialize;

/// Manages the set of pending requests, both incoming and outgoing.
pub struct ReqQueue<I, O> {
    /// The incoming requests.
    pub incoming: Incoming<I>,
    /// The outgoing requests.
    pub outgoing: Outgoing<O>,
}

impl<I, O> Default for ReqQueue<I, O> {
    fn default() -> ReqQueue<I, O> {
        ReqQueue {
            incoming: Incoming {
                pending: HashMap::default(),
            },
            outgoing: Outgoing {
                next_id: 0,
                pending: HashMap::default(),
            },
        }
    }
}

impl<I, O> fmt::Debug for ReqQueue<I, O> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ReqQueue").finish()
    }
}

impl<I, O> ReqQueue<I, O> {
    /// Prints states of the request queue and panics.
    pub fn begin_panic(&self) {
        let keys = self.incoming.pending.keys().cloned().collect::<Vec<_>>();
        log::error!("incoming pending: {keys:?}");
        let keys = self.outgoing.pending.keys().cloned().collect::<Vec<_>>();
        log::error!("outgoing pending: {keys:?}");

        panic!("req queue panicking");
    }
}

/// The incoming request queue.
#[derive(Debug)]
pub struct Incoming<I> {
    pending: HashMap<RequestId, I>,
}

/// The outgoing request queue.
///
/// It holds the next request ID and the pending requests.
#[derive(Debug)]
pub struct Outgoing<O> {
    next_id: i32,
    pending: HashMap<RequestId, O>,
}

impl<I> Incoming<I> {
    /// Registers a request with the given ID and data.
    pub fn register(&mut self, id: RequestId, data: I) {
        self.pending.insert(id, data);
    }

    /// Checks if there are *any* pending requests.
    ///
    /// This is useful for testing language server.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Checks if a request with the given ID is completed.
    pub fn is_completed(&self, id: &RequestId) -> bool {
        !self.pending.contains_key(id)
    }

    /// Cancels a request with the given ID.
    #[cfg(feature = "lsp")]
    pub fn cancel(&mut self, id: RequestId) -> Option<Response> {
        let _data = self.complete(&id)?;
        let error = ResponseError {
            code: ErrorCode::RequestCanceled as i32,
            message: "canceled by client".to_string(),
            data: None,
        };
        Some(Response {
            id,
            result: None,
            error: Some(error),
        })
    }

    /// Completes a request with the given ID.
    pub fn complete(&mut self, id: &RequestId) -> Option<I> {
        self.pending.remove(id)
    }
}

impl<O> Outgoing<O> {
    /// Allocates a request ID.
    pub fn alloc_request_id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Registers a request with the given method, params, and data.
    #[cfg(feature = "lsp")]
    pub fn register<P: Serialize>(&mut self, method: String, params: P, data: O) -> Request {
        let id = RequestId::from(self.alloc_request_id());
        self.pending.insert(id.clone(), data);
        Request::new(id, method, params)
    }

    /// Completes a request with the given ID.
    pub fn complete(&mut self, id: RequestId) -> Option<O> {
        self.pending.remove(&id)
    }
}
