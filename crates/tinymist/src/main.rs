#![doc = include_str!("../README.md")]

mod args;

use std::{
    io::{self, BufRead, Read, Write},
    sync::Arc,
};

use clap::Parser;
use log::{info, trace, warn};
use lsp_types::{InitializeParams, InitializedParams};
use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use tinymist::{init::Init, transport::io_transport, LspHost};
use typst_ts_core::config::CompileOpts;

use crate::args::CliArguments;

// use lsp_types::OneOf;
// use lsp_types::{
//     request::GotoDefinition, GotoDefinitionResponse, InitializeParams,
// ServerCapabilities, };

use lsp_server::{Connection, Message, Response};

fn from_json<T: DeserializeOwned>(
    what: &'static str,
    json: &serde_json::Value,
) -> anyhow::Result<T> {
    serde_json::from_value(json.clone())
        .map_err(|e| anyhow::format_err!("Failed to deserialize {what}: {e}; {json}"))
}

/// The main entry point.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Start logging
    let _ = {
        use log::LevelFilter::*;
        env_logger::builder()
            .filter_module("tinymist", Info)
            .filter_module("typst_preview", Debug)
            .filter_module("typst_ts", Info)
            .filter_module("typst_ts_compiler::service::compile", Info)
            .filter_module("typst_ts_compiler::service::watch", Debug)
            .try_init()
    };

    // Parse command line arguments
    let args = CliArguments::parse();
    info!("Arguments: {:#?}", args);

    match args.mode.as_str() {
        "server" => {}
        "probe" => return Ok(()),
        _ => {
            return Err(anyhow::anyhow!(
                "unknown mode: {mode}, expected one of: server or probe",
                mode = args.mode,
            ));
        }
    }

    // Note that  we must have our logging only write out to stderr.
    info!("starting generic LSP server");

    // Set up input and output
    let mirror = args.mirror.clone();
    let i = move || -> Box<dyn BufRead> {
        if !args.replay.is_empty() {
            // Get input from file
            let file = std::fs::File::open(&args.replay).unwrap();
            let file = std::io::BufReader::new(file);
            Box::new(file)
        } else if mirror.is_empty() {
            // Get input from stdin
            let stdin = std::io::stdin().lock();
            Box::new(stdin)
        } else {
            let file = std::fs::File::create(&mirror).unwrap();
            let stdin = std::io::stdin().lock();
            Box::new(MirrorWriter(stdin, file, std::sync::Once::new()))
        }
    };
    let o = || std::io::stdout().lock();

    // Create the transport. Includes the stdio (stdin and stdout) versions but this
    // could also be implemented to use sockets or HTTP.
    let (sender, receiver, io_threads) = io_transport(i, o);
    let connection = Connection { sender, receiver };

    // todo: ugly code
    let (initialize_id, initialize_params) = match connection.initialize_start() {
        Ok(it) => it,
        Err(e) => {
            log::error!("failed to initialize: {e}");
            if e.channel_is_disconnected() {
                io_threads.join()?;
            }
            return Err(e.into());
        }
    };
    let request_received = std::time::Instant::now();
    trace!("InitializeParams: {initialize_params}");
    let sender = Arc::new(RwLock::new(Some(connection.sender)));
    let host = LspHost::new(sender.clone());

    let req = lsp_server::Request::new(initialize_id, "initialize".to_owned(), initialize_params);
    host.register_request(&req, request_received);
    let lsp_server::Request {
        id: initialize_id,
        params: initialize_params,
        ..
    } = req;

    let initialize_params = from_json::<InitializeParams>("InitializeParams", &initialize_params)?;

    let (mut service, initialize_result) = Init {
        host: host.clone(),
        compile_opts: CompileOpts {
            font_paths: args.font_paths,
            no_system_fonts: args.no_system_fonts,
            ..Default::default()
        },
    }
    .initialize(initialize_params.clone());

    host.respond(match initialize_result {
        Ok(cap) => Response::new_ok(initialize_id, Some(cap)),
        Err(err) => Response::new_err(initialize_id, err.code, err.message),
    });

    #[derive(Debug, Clone, PartialEq)]
    pub struct ProtocolError(String, bool);

    impl ProtocolError {
        pub(crate) fn new(msg: impl Into<String>) -> Self {
            ProtocolError(msg.into(), false)
        }

        pub(crate) fn disconnected() -> ProtocolError {
            ProtocolError("disconnected channel".into(), true)
        }

        /// Whether this error occured due to a disconnected channel.
        pub fn channel_is_disconnected(&self) -> bool {
            self.1
        }
    }

    info!("waiting for initialized notification");
    let initialized_ack = match &connection.receiver.recv() {
        Ok(Message::Notification(n)) if n.method == "initialized" => Ok(()),
        Ok(msg) => Err(ProtocolError::new(format!(
            r#"expected initialized notification, got: {msg:?}"#
        ))),
        Err(e) => {
            log::error!("failed to receive initialized notification: {e}");
            Err(ProtocolError::disconnected())
        }
    };
    if let Err(e) = initialized_ack {
        if e.channel_is_disconnected() {
            io_threads.join()?;
        }
        return Err(anyhow::anyhow!(
            "failed to receive initialized notification: {e:?}"
        ));
    }

    service.initialized(InitializedParams {});

    service.main_loop(connection.receiver)?;

    // Drop it on the main thread
    {
        sender.write().take();
    }
    io_threads.join()?;
    info!("server did shut down");
    Ok(())
}

struct MirrorWriter<R: Read, W: Write>(R, W, std::sync::Once);

impl<R: Read, W: Write> Read for MirrorWriter<R, W> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let res = self.0.read(buf)?;

        if let Err(err) = self.1.write_all(&buf[..res]) {
            self.2.call_once(|| {
                warn!("failed to write to mirror: {err}");
            });
        }

        Ok(res)
    }
}

impl<R: Read + BufRead, W: Write> BufRead for MirrorWriter<R, W> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.0.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.0.consume(amt);
    }
}
