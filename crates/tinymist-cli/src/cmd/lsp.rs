use sync_ls::transport::{MirrorArgs, with_stdio_transport};
use sync_ls::{LspBuilder, LspMessage};
use tinymist::LONG_VERSION;
use tinymist::world::CompileFontArgs;
use tinymist::{RegularInit, ServerState};

use crate::*;

/// Arguments for starting LSP server.
#[derive(Debug, Clone, Default, clap::Parser)]
pub struct LspArgs {
    /// Arguments for mirroring the transport.
    #[clap(flatten)]
    pub mirror: MirrorArgs,
    /// Arguments for font.
    #[clap(flatten)]
    pub font: CompileFontArgs,
}

/// The main entry point for the language server.
pub fn lsp_main(args: LspArgs) -> Result<()> {
    let pairs = LONG_VERSION.trim().split('\n');
    let pairs = pairs
        .map(|e| e.splitn(2, ":").map(|e| e.trim()).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    log::info!("tinymist version information: {pairs:?}");
    log::info!("starting language server: {args:?}");

    let is_replay = !args.mirror.replay.is_empty();
    with_stdio_transport::<LspMessage>(args.mirror.clone(), |conn| {
        let client = client_root(conn.sender);
        ServerState::install_lsp(LspBuilder::new(
            RegularInit {
                client: client.weak().to_typed(),
                font_opts: args.font,
                exec_cmds: Vec::new(),
            },
            client.weak(),
        ))
        .build()
        .start(conn.receiver, is_replay)
    })?;

    log::info!("language server did shut down");
    Ok(())
}
