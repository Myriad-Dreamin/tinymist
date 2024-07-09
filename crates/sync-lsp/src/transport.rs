use async_lsp::{ClientSocket, LspService};
use serde_json::Value as JsonValue;

use crate::io::{get_io, MirrorArgs};

/// Note that we must have our logging only write out to stderr.
pub async fn with_stdio_transport<
    S: LspService<Response = JsonValue, Error = async_lsp::ResponseError>,
>(
    args: MirrorArgs,
    f: impl FnOnce(ClientSocket) -> S,
) -> anyhow::Result<()> {
    // Create the transport. Includes the stdio (stdin and stdout) versions
    // but this could also be implemented to use sockets or HTTP.
    let (i, o) = get_io(args).await;
    let (mainloop, _) = async_lsp::MainLoop::new_server(f);
    Ok(mainloop.run_buffered(i, o).await?)
}

/// Note that we must have our logging only write out to stderr.
pub async fn with_memory_transport<
    S: LspService<Response = JsonValue, Error = async_lsp::ResponseError>,
>(
    args: MirrorArgs,
    f: impl FnOnce(tokio::io::DuplexStream, ClientSocket) -> S,
) -> anyhow::Result<()> {
    // Create the transport. Includes the stdio (stdin and stdout) versions
    // but this could also be implemented to use sockets or HTTP.
    let (_i, o) = get_io(args).await;
    let (w, r) = tokio::io::duplex(128 * 1024);
    let (mainloop, _) = async_lsp::MainLoop::new_server(|sock| f(w, sock));
    Ok(mainloop
        .run_buffered(tokio_util::compat::TokioAsyncReadCompatExt::compat(r), o)
        .await?)
}
