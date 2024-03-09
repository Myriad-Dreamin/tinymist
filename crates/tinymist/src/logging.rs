//! Logging middleware for language server.

use core::fmt;
use core::task::{Context, Poll};
use std::time::Instant;

use futures::future::BoxFuture;
use tower_lsp::jsonrpc::{Request, Response};

/// A middleware that logs requests and responses.
pub struct LogService<S> {
    /// The inner service.
    pub inner: S,
    /// Whether to log the time to process on end of requests.
    pub show_time: bool,
}

impl<S> tower::Service<Request> for LogService<S>
where
    S: tower::Service<Request, Response = Option<Response>>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        struct DisplayId(Option<tower_lsp::jsonrpc::Id>);

        impl fmt::Display for DisplayId {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let Some(id) = &self.0 else { return Ok(()) };
                write!(f, "({id})")
            }
        }

        // Before request.
        let id = DisplayId(request.id().cloned());
        let show_time = self.show_time.then(Instant::now);
        log::info!("request{id}: start {method}", method = request.method());

        let fut = self.inner.call(request);
        Box::pin(async move {
            let response = fut.await?;

            // After request.
            let delta_msg = show_time.map(|s| format!(" in {:?}", s.elapsed()));
            let delta_msg = delta_msg.as_deref().unwrap_or("");
            log::info!("request{id}: finished{delta_msg}");
            Ok(response)
        })
    }
}
