use std::{borrow::Cow, net::SocketAddr, path::Path};

use anyhow::Context;
use await_tree::InstrumentAwait;
use log::{error, info};

use typst::foundations::{Str, Value};
use typst_ts_core::config::{compiler::EntryOpts, CompileOpts};

use hyper::{
    service::{make_service_fn, service_fn},
    Error,
};

use tinymist_assets::TYPST_PREVIEW_HTML;
use typst_preview::{
    await_tree::{get_await_tree_async, REGISTRY},
    preview, PreviewArgs, PreviewMode, Previewer,
};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "clap", derive(clap::Parser))]
pub struct PreviewCliArgs {
    #[cfg_attr(feature = "clap", clap(flatten))]
    pub preview: PreviewArgs,

    #[cfg_attr(feature = "clap", clap(flatten))]
    pub compile: CompileOnceArgs,

    /// Preview mode
    #[cfg_attr(
        feature = "clap",
        clap(long = "preview-mode", default_value = "document", value_name = "MODE")
    )]
    pub preview_mode: PreviewMode,

    /// Host for the preview server
    #[cfg_attr(
        feature = "clap",
        clap(
            long = "host",
            value_name = "HOST",
            default_value = "127.0.0.1:23627",
            alias = "static-file-host"
        )
    )]
    pub static_file_host: String,

    /// Don't open the preview in the browser after compilation.
    #[cfg_attr(feature = "clap", clap(long = "no-open"))]
    pub dont_open_in_browser: bool,
}

#[path = "preview_compiler.rs"]
mod compiler;
use compiler::CompileServer;

use crate::{compile_init::CompileOnceArgs, LspUniverse};

pub fn make_static_host(
    previewer: &Previewer,
    static_file_addr: String,
    mode: PreviewMode,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let frontend_html = previewer.frontend_html(mode);
    let make_service = make_service_fn(move |_| {
        let html = frontend_html.clone();
        async move {
            Ok::<_, hyper::http::Error>(service_fn(move |req| {
                // todo: clone may not be necessary
                let html = html.as_ref().to_owned();
                async move {
                    if req.uri().path() == "/" {
                        log::info!("Serve frontend: {:?}", mode);
                        Ok::<_, Error>(hyper::Response::new(hyper::Body::from(html)))
                    } else if req.uri().path() == "/await_tree" {
                        Ok::<_, Error>(hyper::Response::new(hyper::Body::from(
                            get_await_tree_async().await,
                        )))
                    } else {
                        // jump to /
                        let mut res = hyper::Response::new(hyper::Body::empty());
                        *res.status_mut() = hyper::StatusCode::FOUND;
                        res.headers_mut().insert(
                            hyper::header::LOCATION,
                            hyper::header::HeaderValue::from_static("/"),
                        );
                        Ok(res)
                    }
                }
            }))
        }
    });
    let server = hyper::Server::bind(&static_file_addr.parse().unwrap()).serve(make_service);

    let addr = server.local_addr();
    let join_handle = tokio::spawn(async move {
        if let Err(e) = server.await {
            error!("Static file server error: {}", e);
        }
    });
    (addr, join_handle)
}

/// Entry point.
pub async fn preview_main(args: PreviewCliArgs) -> anyhow::Result<()> {
    let async_root = REGISTRY
        .lock()
        .await
        .register("root".into(), "typst-preview");
    info!("Arguments: {:#?}", args);
    let input = args.compile.input.context("entry file must be provided")?;
    let input = Path::new(&input);
    let entry = if input.is_absolute() {
        input.to_owned()
    } else {
        std::env::current_dir().unwrap().join(input)
    };
    let inputs = args
        .compile
        .inputs
        .iter()
        .map(|(k, v)| (Str::from(k.as_str()), Value::Str(Str::from(v.as_str()))))
        .collect();
    let root = if let Some(root) = &args.compile.root {
        if root.is_absolute() {
            root.clone()
        } else {
            std::env::current_dir().unwrap().join(root)
        }
    } else {
        std::env::current_dir().unwrap()
    };
    if !entry.starts_with(&root) {
        error!("entry file must be in the root directory");
        std::process::exit(1);
    }

    let world = {
        let world = LspUniverse::new(CompileOpts {
            entry: EntryOpts::new_rooted(root.clone(), Some(entry.clone())),
            inputs,
            no_system_fonts: args.compile.font.ignore_system_fonts,
            font_paths: args.compile.font.font_paths.clone(),
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
            ..CompileOpts::default()
        })
        .expect("incorrect options");

        world.with_entry_file(entry)
    };

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("Ctrl-C received, exiting");
        std::process::exit(0);
    });

    let previewer = preview(
        args.preview,
        move |handle| {
            let compile_server = CompileServer::new(world, handle);

            compile_server.spawn().unwrap()
        },
        TYPST_PREVIEW_HTML,
    );
    let previewer = async_root
        .instrument(previewer)
        .instrument_await("preview")
        .await;

    let static_file_addr = args.static_file_host;
    let mode = args.preview_mode;
    let (static_server_addr, static_server_handle) =
        make_static_host(&previewer, static_file_addr, mode);
    info!("Static file server listening on: {}", static_server_addr);
    if !args.dont_open_in_browser {
        if let Err(e) = open::that_detached(format!("http://{}", static_server_addr)) {
            error!("failed to open browser: {}", e);
        };
    }
    let _ = tokio::join!(previewer.join(), static_server_handle);

    Ok(())
}
