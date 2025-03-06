//! The actor that runs user actions.

use std::path::PathBuf;

use anyhow::bail;
use base64::Engine;
use hyper::service::service_fn;
use hyper_util::{rt::TokioIo, server::graceful::GracefulShutdown};
use lsp_server::RequestId;
use reflexo_typst::{TypstDict, TypstPagedDocument};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sync_lsp::{just_future, LspClient, LspResult, SchedulableResponse};
use tinymist_project::LspWorld;
use tinymist_std::error::IgnoreLogging;
use tokio::sync::{oneshot, watch};
use typst::{syntax::Span, World};

use crate::{internal_error, ServerState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceParams {
    pub compiler_program: PathBuf,
    pub root: PathBuf,
    pub main: PathBuf,
    pub inputs: TypstDict,
    pub font_paths: Vec<PathBuf>,
    pub rpc_kind: String,
}

/// The user action task.
#[derive(Default, Clone, Copy)]
pub struct UserActionTask;

impl UserActionTask {
    /// Traces a specific document.
    pub fn trace_document(&self, params: TraceParams) -> SchedulableResponse<JsonValue> {
        just_future(async move {
            run_trace_program(params)
                .await
                .map_err(|e| internal_error(format!("failed to run trace program: {e:?}")))
        })
    }

    /// Traces the entire server.
    pub fn trace_server(&self) -> (ServerTraceTask, SchedulableResponse<JsonValue>) {
        let (stop_tx, mut stop_rx) = watch::channel(false);
        let mut stop_rx2 = stop_rx.clone();

        let (resp_tx, resp_rx) = oneshot::channel();
        let task = ServerTraceTask { stop_tx, resp_rx };

        typst_timing::enable();

        let resp = just_future(async move {
            let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();
            let t = tokio::spawn(async move {

                log::info!("before generate timings");

                let timings = async {

                    stop_rx.wait_for(|stopped| *stopped).await.ok();
                    typst_timing::disable();

                    let mut writer = std::io::BufWriter::new(Vec::new());
                    // todo: resolve span correctly
                    let _ = typst_timing::export_json(&mut writer, |_| {
                        ("unknown".to_string(), 0)
                    });

                    let timings = writer.into_inner().unwrap();

                    // let timings_debug = serde_json::from_slice::<serde_json::Value>(&timings).unwrap();
                    // log::info!("timings: {:?}", timings_debug);

                    timings

                }.await;

                log::info!("after generate timings");
                //log::info!("timings: {:?}", timings);

                // let _ = resp_tx;
                // let res = serde_json::to_value(TraceReport {
                //     request: params,
                //     messages,
                //     stderr: base64::engine::general_purpose::STANDARD.encode(String::new()),
                // })?;

                log::info!("now make http server");

                let static_file_addr = "127.0.0.1:0".to_owned();
                make_http_server(timings, static_file_addr, addr_tx).await;
            });

            let addr = addr_rx.await.map_err(|err| {
                log::error!("failed to get address of trace server: {err:?}");
                internal_error("failed to get address of trace server")
            })?;
            
            log::info!("trace server has started at {addr}");

            tokio::spawn(async move {

                let selected = tokio::select! {
                    a = stop_rx2.wait_for(|stopped| *stopped) => {
                        log::info!("trace server task stopped by user");
                        Ok(a)
                    },
                    b = t => {
                        log::info!("trace server task stopped by timeout");
                        Err(b)
                    },
                };

                match selected {
                    Ok(Err(err)) => {
                        log::error!("Error occurs when trace server task stopped by user: {err:?}");
                    }
                    Err(Err(err)) => {
                        log::error!("occurs when trace server task stopped by timeout: {err:?}");
                    }
                    Ok(Ok(_)) | Err(Ok(_)) => {}
                };

                resp_tx
                    .send(Ok(json!({
                        "tracingUrl": format!("http://{addr}"),  // send attr to stopServerProfiling message processor directly
                    })))
                    .ok()
                    .log_error("failed to send response");
            });

            Ok(serde_json::json!({
                "tracingUrl": format!("http://{addr}"),  // not used
            }))
        });

        (task, resp)
    }

    /// Run a trace request in subprocess.
    pub async fn trace_main(
        client: LspClient,
        state: &mut ServerState,
        w: &LspWorld,
        rpc_kind: String,
        req_id: RequestId,
    ) -> ! {
        trace_main(client, state, w, rpc_kind, req_id).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceReport {
    request: TraceParams,
    messages: Vec<lsp_server::Message>,
    stderr: String,
}

/// Run a perf trace to some typst program
async fn run_trace_program(params: TraceParams) -> anyhow::Result<JsonValue> {
    // Typst compile root, input, font paths, inputs
    let mut cmd = std::process::Command::new(&params.compiler_program);
    let mut cmd = &mut cmd;

    cmd = cmd.arg("trace-lsp");

    cmd = cmd
        .arg("--rpc-kind")
        .arg(&params.rpc_kind)
        .arg("--root")
        .arg(params.root.as_path())
        .arg(params.main.as_path());

    // todo: test space in input?
    for (k, v) in params.inputs.iter() {
        let typst::foundations::Value::Str(s) = v else {
            bail!("input value must be string, got {v:?} for {k:?}");
        };
        cmd = cmd.arg(format!("--input={k}={}", s.as_str()));
    }
    for p in &params.font_paths {
        cmd = cmd.arg(format!("--font-path={}", p.as_path().display()));
    }

    log::info!("running trace program: {cmd:?}");
    let start = reflexo::time::Instant::now();

    // FIXME: we actually have waited it by `wait_with_output`
    #[allow(clippy::zombie_processes)]
    let mut child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("trace program command failed to start");

    let stdout = child.stdout.take().expect("stdout missing");

    let (msg_tx, msg_rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let mut input_chan = std::io::BufReader::new(stdout);
        let mut has_response = false;
        let messages = std::iter::from_fn(|| {
            if has_response {
                return None;
            }
            let msg = lsp_server::Message::read(&mut input_chan).ok()?;
            if let Some(lsp_server::Message::Response(resp)) = &msg {
                if resp.id == 0.into() {
                    has_response = true;
                }
            }
            Some(msg)
        })
        .flatten()
        .collect::<Vec<_>>();
        msg_tx.send(messages).ok();
    });

    std::thread::spawn(move || {
        let res = child.wait_with_output();
        match res {
            Ok(output) => {
                log::info!("trace program exited with status: {:?}", output.status);

                use std::io::BufRead;
                for line in output.stderr.lines() {
                    let Ok(line) = line else {
                        continue;
                    };
                    log::error!("trace program stderr: {line}");
                }
            }
            Err(e) => {
                log::error!("trace program failed with error: {e:?}");
            }
        }
    });

    let messages = msg_rx.await.unwrap();

    log::info!("trace program executed in {:?}", start.elapsed());
    let start = reflexo::time::Instant::now();

    let res = serde_json::to_value(TraceReport {
        request: params,
        messages,
        stderr: base64::engine::general_purpose::STANDARD.encode(String::new()),
    })?;
    log::info!("trace result encoded in {:?}", start.elapsed());

    Ok(res)
}

async fn trace_main(
    client: LspClient,
    state: &mut ServerState,
    w: &LspWorld,
    rpc_kind: String,
    req_id: RequestId,
) -> ! {
    typst_timing::enable();
    let res = typst::compile::<TypstPagedDocument>(w);
    let diags = match &res.output {
        Ok(_res) => res.warnings,
        Err(errors) => errors.clone(),
    };
    let mut writer = std::io::BufWriter::new(Vec::new());
    let _ = typst_timing::export_json(&mut writer, |span| {
        resolve_span(w, Span::from_raw(span)).unwrap_or_else(|| ("unknown".to_string(), 0))
    });

    let timings = writer.into_inner().unwrap();

    let handle = &state.project;
    let diagnostics =
        tinymist_query::convert_diagnostics(w, diags.iter(), handle.analysis.position_encoding);

    let rpc_kind = rpc_kind.as_str();

    client.send_notification_(lsp_server::Notification {
        method: "tinymistExt/diagnostics".to_owned(),
        params: serde_json::json!(diagnostics),
    });
    match rpc_kind {
        "lsp" => {
            client.respond(lsp_server::Response {
                id: req_id,
                result: Some(serde_json::json!({
                    "tracingData": String::from_utf8(timings).unwrap(),
                })),
                error: None,
            });
        }
        "http" => {
            let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();
            let t = tokio::spawn(async move {
                let static_file_addr = "127.0.0.1:0".to_owned();
                make_http_server(timings, static_file_addr, addr_tx).await;
                std::process::exit(0);
            });

            let addr = addr_rx.await.unwrap();

            client.respond(lsp_server::Response {
                id: req_id,
                result: Some(serde_json::json!({
                    "tracingUrl": format!("http://{addr}"),
                })),
                error: None,
            });

            t.await.unwrap();
        }
        kind => {
            panic!("unsupported rpc kind: {kind:?}");
        }
    }

    std::process::exit(0);
}

/// The server trace task.
pub struct ServerTraceTask {
    /// The sender to stop the trace.
    pub stop_tx: watch::Sender<bool>,
    /// The receiver to get the trace result.
    pub resp_rx: oneshot::Receiver<LspResult<JsonValue>>,
}

// todo: reuse code from tools preview
/// Create a http server for the trace program.
async fn make_http_server(
    timings: Vec<u8>,
    static_file_addr: String,
    addr_tx: tokio::sync::oneshot::Sender<std::net::SocketAddr>,
) {
    use http_body_util::Full;
    use hyper::body::{Bytes, Incoming};
    type Server = hyper_util::server::conn::auto::Builder<hyper_util::rt::TokioExecutor>;

    let (alive_tx, mut alive_rx) = tokio::sync::mpsc::unbounded_channel();
    let timings = hyper::body::Bytes::from(timings);
    let make_service = move || {
        let timings = timings.clone();
        let alive_tx = alive_tx.clone();
        service_fn(move |req: hyper::Request<Incoming>| {
            let timings = timings.clone();
            let _ = alive_tx.send(());
            async move {
                // Make sure VSCode can connect to this http server but no malicious website a
                // user might open in a browser. We recognize VSCode by an `Origin` header that
                // starts with `vscode-webview://`. Malicious websites can (hopefully) not trick
                // browsers into sending an `Origin` header that starts with
                // `vscode-webview://`.
                //
                // See comment in `make_http_server` in `crates/tinymist/src/tool/preview.rs`
                // for more details. In particular, note that this does _not_ protect against
                // malicious users that share the same computer as us.
                let Some(allowed_origin) = req
                    .headers()
                    .get("Origin")
                    .filter(|h| h.as_bytes().starts_with(b"vscode-webview://"))
                else {
                    anyhow::bail!("Origin must start with vscode-webview://");
                };

                let b = hyper::Response::builder()
                    .header(hyper::header::ACCESS_CONTROL_ALLOW_ORIGIN, allowed_origin);
                if req.uri().path() == "/" {
                    let res = if req.method() == hyper::Method::HEAD {
                        b.body(Full::<Bytes>::default()).unwrap()
                    } else {
                        b.header(hyper::header::CONTENT_TYPE, "application/json")
                            .body(Full::<Bytes>::from(timings))
                            .unwrap()
                    };

                    Ok::<_, anyhow::Error>(res)
                } else {
                    // jump to /
                    let res = b
                        .status(hyper::StatusCode::FOUND)
                        .header(hyper::header::LOCATION, "/")
                        .body(Full::<Bytes>::default())
                        .unwrap();
                    Ok(res)
                }
            }
        })
    };

    let listener = tokio::net::TcpListener::bind(&static_file_addr)
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    log::info!("trace server listening on http://{addr}");

    let (final_tx, final_rx) = tokio::sync::oneshot::channel();

    // the graceful watcher
    let graceful = hyper_util::server::graceful::GracefulShutdown::new();

    let serve_conn = move |server: &Server, graceful: &GracefulShutdown, conn| {
        let (stream, _peer_addr) = match conn {
            Ok(conn) => conn,
            Err(e) => {
                log::error!("accept error: {e}");
                return;
            }
        };

        let conn = server.serve_connection(TokioIo::new(stream), make_service());
        let conn = graceful.watch(conn.into_owned());
        tokio::spawn(async move {
            conn.await.log_error("cannot serve http");
        });
    };

    let join = tokio::spawn(async move {
        // when this signal completes, start shutdown
        let mut signal = std::pin::pin!(final_rx);

        let mut server = Server::new(hyper_util::rt::TokioExecutor::new());
        server.http1().keep_alive(true);

        loop {
            tokio::select! {
                conn = listener.accept() => serve_conn(&server, &graceful, conn),
                Ok(_) = &mut signal => {
                    log::info!("graceful shutdown signal received");
                    break;
                }
            }
        }

        tokio::select! {
            _ = graceful.shutdown() => {
                log::info!("gracefully shutdown!");
            },
            _ = tokio::time::sleep(reflexo::time::Duration::from_secs(10)) => {
                log::info!("waited 10 seconds for graceful shutdown, aborting...");
            }
        }
    });
    // final_tx.send(()).ok();

    tokio::spawn(async move {
        // timeout alive_rx
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    log::info!("trace-server: ctrl-c received, shutting down");
                    final_tx.send(()).ok();
                    break;
                },
                _ = tokio::time::sleep(reflexo::time::Duration::from_secs(15)) => {
                    log::info!("trace-server: No activity for 15 seconds, shutting down");
                    final_tx.send(()).ok();
                    break;
                },
                _ = alive_rx.recv() => {
                    log::info!("trace-server: Activity detected, resetting timer");
                }
            }
        }
    });

    addr_tx.send(addr).ok();
    join.await.unwrap();
}

/// Turns a span into a (file, line) pair.
fn resolve_span(world: &LspWorld, span: Span) -> Option<(String, u32)> {
    let id = span.id()?;
    let source = world.source(id).ok()?;
    let range = source.range(span)?;
    let line = source.byte_to_line(range.start)?;
    Some((format!("{id:?}"), line as u32 + 1))
}
