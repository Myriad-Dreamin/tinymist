use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use hyper_tungstenite::tungstenite::Message;
use tinymist::{
    project::ProjectPreviewState,
    tool::{
        preview::{PreviewCliArgs, ProjectPreviewHandler, bind_streams, make_http_server},
        project::{ProjectOpts, StartProjectResult, start_project},
    },
};
use tinymist_assets::TYPST_PREVIEW_HTML;
use tinymist_preview::{
    ControlPlaneMessage, ControlPlaneTx, PreviewBuilder, PreviewConfig, frontend_html,
};
use tinymist_project::WorldProvider;
use tinymist_std::error::prelude::*;
use tokio::sync::mpsc;

use crate::utils::exit_on_ctrl_c;

/// Entry point of the preview tool.
pub async fn preview_main(args: PreviewCliArgs) -> Result<()> {
    log::info!("Arguments: {args:#?}");
    let handle = tokio::runtime::Handle::current();

    let config = args.preview.config(&PreviewConfig::default());
    #[cfg(feature = "open")]
    let open_in_browser = args.open_in_browser(true);
    let static_file_host =
        if args.static_file_host == args.data_plane_host || !args.static_file_host.is_empty() {
            Some(args.static_file_host)
        } else {
            None
        };

    exit_on_ctrl_c();

    let verse = args.compile.resolve()?;
    let previewer = PreviewBuilder::new(config);

    let (service, handle) = {
        let preview_state = ProjectPreviewState::default();
        let opts = ProjectOpts {
            handle: Some(handle),
            preview: preview_state.clone(),
            ..ProjectOpts::default()
        };

        let StartProjectResult {
            service,
            intr_tx,
            mut editor_rx,
        } = start_project(verse, Some(opts), |compiler, intr, next| {
            next(compiler, intr)
        });

        // Consume editor_rx
        tokio::spawn(async move { while editor_rx.recv().await.is_some() {} });

        let id = service.compiler.primary.id.clone();
        let registered = preview_state.register(&id, previewer.compile_watcher(args.task_id));
        if !registered {
            tinymist_std::bail!("failed to register preview");
        }

        let handle: Arc<ProjectPreviewHandler> = Arc::new(ProjectPreviewHandler {
            project_id: id,
            client: Box::new(intr_tx),
        });

        (service, handle)
    };

    let (lsp_tx, mut lsp_rx) = ControlPlaneTx::new(true);

    let control_plane_server_handle = tokio::spawn(async move {
        let (control_sock_tx, mut control_sock_rx) = mpsc::unbounded_channel();

        let srv =
            make_http_server(String::default(), args.control_plane_host, control_sock_tx).await;
        log::info!("Control panel server listening on: {}", srv.addr);

        let control_websocket = control_sock_rx.recv().await.unwrap();
        let ws = control_websocket.await.unwrap();

        tokio::pin!(ws);

        loop {
            tokio::select! {
                Some(resp) = lsp_rx.resp_rx.recv() => {
                    let r = ws
                        .send(Message::text(serde_json::to_string(&resp).unwrap()))
                        .await;
                    let Err(err) = r else {
                        continue;
                    };

                    log::warn!("failed to send response to editor {err:?}");
                    break;

                }
                msg = ws.next() => {
                    let msg = match msg {
                        Some(Ok(Message::Text(msg))) => Some(msg),
                        Some(Ok(msg)) => {
                            log::error!("unsupported message: {msg:?}");
                            break;
                        }
                        Some(Err(e)) => {
                            log::error!("failed to receive message: {e}");
                            break;
                        }
                        _ => None,
                    };

                    if let Some(msg) = msg {
                        let Ok(msg) = serde_json::from_str::<ControlPlaneMessage>(&msg) else {
                            log::warn!("failed to parse control plane request: {msg:?}");
                            break;
                        };

                        lsp_rx.ctl_tx.send(msg).unwrap();
                    } else {
                        // todo: inform the editor that the connection is closed.
                        break;
                    }
                }

            }
        }

        let _ = srv.shutdown_tx.send(());
        let _ = srv.join.await;
    });

    let (websocket_tx, websocket_rx) = mpsc::unbounded_channel();
    let mut previewer = previewer.build(lsp_tx, handle.clone()).await;
    tokio::spawn(service.run());

    bind_streams(&mut previewer, websocket_rx);

    let frontend_html = frontend_html(TYPST_PREVIEW_HTML, args.preview.preview_mode, "/");

    let static_server = if let Some(static_file_host) = static_file_host {
        log::warn!(
            "--static-file-host is deprecated, which will be removed in the future. Use --data-plane-host instead."
        );
        let html = frontend_html.clone();
        Some(make_http_server(html, static_file_host, websocket_tx.clone()).await)
    } else {
        None
    };

    let srv = make_http_server(frontend_html, args.data_plane_host, websocket_tx).await;
    log::info!("Data plane server listening on: {}", srv.addr);

    let static_server_addr = static_server.as_ref().map(|s| s.addr).unwrap_or(srv.addr);
    log::info!("Static file server listening on: {static_server_addr}");

    #[cfg(feature = "open")]
    if open_in_browser {
        open::that_detached(format!("http://{static_server_addr}"))
            .log_error("failed to open browser for preview");
    }

    let _ = tokio::join!(previewer.join(), srv.join, control_plane_server_handle);
    // Assert that the static server's lifetime is longer than the previewer.
    let _s = static_server;

    Ok(())
}
