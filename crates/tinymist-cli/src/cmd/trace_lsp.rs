use std::path::PathBuf;

use futures::future::MaybeDone;
use reflexo::ImmutPath;
use sync_ls::transport::{MirrorArgs, with_stdio_transport};
use sync_ls::{LspBuilder, LspMessage, RequestId};
use tinymist::world::TaskInputs;
use tinymist::{CompileOnceArgs, Config, ServerState, SuperInit, UserActionTask};
use tinymist_project::EntryResolver;
use tinymist_std::{bail, error::prelude::*};

use crate::*;

#[derive(Debug, Clone, Default, clap::Parser)]
pub struct TraceLspArgs {
    #[clap(long, default_value = "false")]
    pub persist: bool,
    // lsp or http
    #[clap(long, default_value = "lsp")]
    pub rpc_kind: String,
    #[clap(flatten)]
    pub mirror: MirrorArgs,
    #[clap(flatten)]
    pub compile: CompileOnceArgs,
}

/// The main entry point for the compiler.
pub fn trace_lsp_main(args: TraceLspArgs) -> Result<()> {
    let inputs = args.compile.resolve_inputs();
    let mut input = PathBuf::from(match args.compile.input {
        Some(value) => value,
        None => Err(anyhow::anyhow!("provide a valid path"))?,
    });
    let mut root_path = args.compile.root.unwrap_or(PathBuf::from("."));

    if root_path.is_relative() {
        root_path = std::env::current_dir().context("cwd")?.join(root_path);
    }
    if input.is_relative() {
        input = std::env::current_dir().context("cwd")?.join(input);
    }
    if !input.starts_with(&root_path) {
        bail!("input file is not within the root path: {input:?} not in {root_path:?}");
    }

    with_stdio_transport::<LspMessage>(args.mirror.clone(), |conn| {
        let client_root = client_root(conn.sender);
        let client = client_root.weak();
        let roots = vec![ImmutPath::from(root_path)];
        let config = Config {
            entry_resolver: EntryResolver {
                roots,
                ..EntryResolver::default()
            },
            font_opts: args.compile.font,
            ..Config::default()
        };

        let mut service = ServerState::install_lsp(LspBuilder::new(
            SuperInit {
                client: client.to_typed(),
                exec_cmds: Vec::new(),
                config,
                err: None,
            },
            client.clone(),
        ))
        .build();

        let resp = service.ready(()).unwrap();
        let MaybeDone::Done(resp) = resp else {
            anyhow::bail!("internal error: not sync init")
        };
        resp.unwrap();

        // todo: persist
        let request_received = reflexo::time::Time::now();

        let req_id: RequestId = 0.into();
        client.register_request("tinymistExt/documentProfiling", &req_id, request_received);

        let state = service.state_mut().unwrap();

        let entry = state.entry_resolver().resolve(Some(input.as_path().into()));

        let snap = state.snapshot().unwrap();

        RUNTIMES.tokio_runtime.block_on(async {
            let g = snap.task(TaskInputs {
                entry: Some(entry),
                inputs,
            });

            UserActionTask::trace_main(client, state, g, args.rpc_kind, req_id).await
        });

        Ok(())
    })?;

    Ok(())
}
