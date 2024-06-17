use std::{borrow::Cow, net::SocketAddr};

use await_tree::InstrumentAwait;
use clap::Parser;
use log::{error, info};

use typst::foundations::{Str, Value};
use typst_ts_compiler::service::CompileDriver;
use typst_ts_compiler::TypstSystemWorld;
use typst_ts_core::config::{compiler::EntryOpts, CompileOpts};

use crate::compiler::CompileServer;

use hyper::{
    service::{make_service_fn, service_fn},
    Error,
};

use tinymist_assets::TYPST_PREVIEW_HTML;
use typst_preview::{
    await_tree::{get_await_tree_async, REGISTRY},
    preview, CliArguments, PreviewMode, Previewer,
};

mod compiler;

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
#[tokio::main]
async fn main() {
    let _ = env_logger::builder()
        // TODO: set this back to Info
        .filter_module("typst_preview", log::LevelFilter::Debug)
        .filter_module("typst_ts", log::LevelFilter::Info)
        // TODO: set this back to Info
        .filter_module(
            "typst_ts_compiler::service::compile",
            log::LevelFilter::Debug,
        )
        .filter_module("typst_ts_compiler::service::watch", log::LevelFilter::Debug)
        .try_init();
    let async_root = REGISTRY
        .lock()
        .await
        .register("root".into(), "typst-preview");
    let arguments = CliArguments::parse();
    info!("Arguments: {:#?}", arguments);
    let entry = if arguments.input.is_absolute() {
        arguments.input.clone()
    } else {
        std::env::current_dir().unwrap().join(&arguments.input)
    };
    let inputs = arguments
        .inputs
        .iter()
        .map(|(k, v)| (Str::from(k.as_str()), Value::Str(Str::from(v.as_str()))))
        .collect();
    let root = if let Some(root) = &arguments.root {
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

    let compiler_driver = {
        let world = TypstSystemWorld::new(CompileOpts {
            entry: EntryOpts::new_rooted(root.clone(), Some(entry.clone())),
            inputs,
            no_system_fonts: arguments.ignore_system_fonts,
            font_paths: arguments.font_paths.clone(),
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
            ..CompileOpts::default()
        })
        .expect("incorrect options");

        CompileDriver::new(world).with_entry_file(entry)
    };

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("Ctrl-C received, exiting");
        std::process::exit(0);
    });

    let previewer = preview(
        arguments.preview,
        move |handle| {
            let compile_server = CompileServer::new(compiler_driver, handle);

            compile_server.spawn().unwrap()
        },
        TYPST_PREVIEW_HTML,
    );
    let previewer = async_root
        .instrument(previewer)
        .instrument_await("preview")
        .await;

    let static_file_addr = arguments.static_file_host;
    let mode = arguments.preview_mode;
    let (static_server_addr, static_server_handle) =
        make_static_host(&previewer, static_file_addr, mode);
    info!("Static file server listening on: {}", static_server_addr);
    if !arguments.dont_open_in_browser {
        if let Err(e) = open::that_detached(format!("http://{}", static_server_addr)) {
            error!("failed to open browser: {}", e);
        };
    }
    let _ = tokio::join!(previewer.join(), static_server_handle);
}
