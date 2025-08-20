use sync_ls::transport::with_stdio_transport;
use sync_ls::{DapBuilder, DapMessage};
use tinymist::LONG_VERSION;
use tinymist::ServerState;
use tinymist_std::error::prelude::*;

use crate::*;

pub type DapArgs = crate::lsp::LspArgs;

/// The main entry point for the language server.
pub fn dap_main(args: DapArgs) -> Result<()> {
    let pairs = LONG_VERSION.trim().split('\n');
    let pairs = pairs
        .map(|e| e.splitn(2, ":").map(|e| e.trim()).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    log::info!("tinymist version information: {pairs:?}");
    log::info!("starting debug adaptor: {args:?}");

    let is_replay = !args.mirror.replay.is_empty();
    with_stdio_transport::<DapMessage>(args.mirror.clone(), |conn| {
        let client = client_root(conn.sender);
        ServerState::install_dap(DapBuilder::new(
            tinymist::DapRegularInit {
                client: client.weak().to_typed(),
                font_opts: args.font,
            },
            client.weak(),
        ))
        .build()
        .start(conn.receiver, is_replay)
    })?;

    log::info!("language server did shut down");
    Ok(())
}
