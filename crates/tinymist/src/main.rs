//! # tinymist LSP Server

use core::fmt;
use core::task::{Context, Poll};
use std::time::Instant;

use futures::future::BoxFuture;
use tinymist::TypstServer;
use tower_lsp::{
    jsonrpc::{Request, Response},
    LspService, Server,
};

// #[derive(Debug, Clone)]
// struct Args {}

// fn arg_parser() -> OptionParser<Args> {
//     construct!(Args {}).to_options().version(
//         format!(
//             "{}, commit {} (Typst version {TYPST_VERSION})",
//             env!("CARGO_PKG_VERSION"),
//             env!("GIT_COMMIT")
//         )
//         .as_str(),
//     )
// }

// pub const TYPST_VERSION: &str = env!("TYPST_VERSION");

#[tokio::main]
async fn main() {
    let _ = env_logger::builder()
        .filter_module("tinymist", log::LevelFilter::Info)
        .filter_module("typst_preview", log::LevelFilter::Debug)
        .filter_module("typst_ts", log::LevelFilter::Info)
        // TODO: set this back to Info
        .filter_module(
            "typst_ts_compiler::service::compile",
            log::LevelFilter::Info,
        )
        .filter_module("typst_ts_compiler::service::watch", log::LevelFilter::Debug)
        .try_init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (inner, socket) = LspService::new(TypstServer::new);

    Server::new(stdin, stdout, socket)
        .serve(LogService {
            inner,
            show_time: true,
        })
        .await;
}

struct LogService<S> {
    inner: S,
    show_time: bool,
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
                write!(f, "({})", id)
            }
        }

        let id = DisplayId(request.id().cloned());
        let method = request.method();
        let show_time = self.show_time.then(Instant::now);
        log::info!("request{id}: start {method}");

        let fut = self.inner.call(request);
        Box::pin(async move {
            let response = fut.await?;

            let delta_msg = show_time.map(|s| format!(" in {:?}", s.elapsed()));
            let delta_msg = delta_msg.as_deref().unwrap_or("");
            log::info!("request{id}: finished{delta_msg}");
            Ok(response)
        })
    }
}
