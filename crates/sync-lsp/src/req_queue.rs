use core::fmt;
use std::collections::HashMap;

use serde::Serialize;

use lsp_server::{ErrorCode, Request, RequestId, Response, ResponseError};

/// Manages the set of pending requests, both incoming and outgoing.
pub struct ReqQueue<I, O> {
    pub incoming: Incoming<I>,
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
    pub fn begin_panic(&self) {
        let keys = self.incoming.pending.keys().cloned().collect::<Vec<_>>();
        log::error!("incoming pending: {keys:?}");
        let keys = self.outgoing.pending.keys().cloned().collect::<Vec<_>>();
        log::error!("outgoing pending: {keys:?}");

        panic!("req queue panicking");
    }
}

#[derive(Debug)]
pub struct Incoming<I> {
    pending: HashMap<RequestId, I>,
}

#[derive(Debug)]
pub struct Outgoing<O> {
    next_id: i32,
    pending: HashMap<RequestId, O>,
}

impl<I> Incoming<I> {
    pub fn register(&mut self, id: RequestId, data: I) {
        self.pending.insert(id, data);
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn cancel(&mut self, id: RequestId) -> Option<Response> {
        let _data = self.complete(id.clone())?;
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

    pub fn complete(&mut self, id: RequestId) -> Option<I> {
        self.pending.remove(&id)
    }

    pub fn is_completed(&self, id: &RequestId) -> bool {
        !self.pending.contains_key(id)
    }
}

impl<O> Outgoing<O> {
    pub fn register<P: Serialize>(&mut self, method: String, params: P, data: O) -> Request {
        let id = RequestId::from(self.next_id);
        self.pending.insert(id.clone(), data);
        self.next_id += 1;
        Request::new(id, method, params)
    }

    pub fn complete(&mut self, id: RequestId) -> Option<O> {
        self.pending.remove(&id)
    }
}
