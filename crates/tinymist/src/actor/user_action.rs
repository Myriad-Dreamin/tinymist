use std::path::PathBuf;

use anyhow::bail;
use base64::Engine;
use lsp_server::RequestId;
use serde::{Deserialize, Serialize};
use typst_ts_core::TypstDict;

use crate::{internal_error, result_to_response_, LspHost, TypstLanguageServer};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserActionTraceRequest {
    #[serde(rename = "compilerProgram")]
    pub compiler_program: PathBuf,
    pub root: PathBuf,
    pub main: PathBuf,
    pub inputs: TypstDict,
    #[serde(rename = "fontPaths")]
    pub font_paths: Vec<PathBuf>,
}

pub enum UserActionRequest {
    Trace((RequestId, UserActionTraceRequest)),
}

pub fn run_user_action_thread(
    rx_req: crossbeam_channel::Receiver<UserActionRequest>,
    client: LspHost<TypstLanguageServer>,
) {
    while let Ok(req) = rx_req.recv() {
        match req {
            UserActionRequest::Trace((id, req)) => {
                let res = run_trace_program(req)
                    .map_err(|e| internal_error(format!("failed to run trace program: {:?}", e)));

                if let Ok(response) = result_to_response_(id, res) {
                    client.respond(response);
                }
            }
        }
    }

    log::info!("Trace thread did shut down");
}

/// Run a perf trace to some typst program
fn run_trace_program(req: UserActionTraceRequest) -> anyhow::Result<TraceReport> {
    // Typst compile root, input, font paths, inputs
    let mut cmd = std::process::Command::new(&req.compiler_program);
    let mut cmd = &mut cmd;

    cmd = cmd.arg("compile");

    cmd = cmd
        .arg("--root")
        .arg(req.root.as_path())
        .arg(req.main.as_path());

    // todo: test space in input?
    for (k, v) in req.inputs.iter() {
        let typst::foundations::Value::Str(s) = v else {
            bail!("input value must be string, got {:?} for {:?}", v, k);
        };
        cmd = cmd.arg(format!("--input={k}={}", s.as_str()));
    }
    for p in &req.font_paths {
        cmd = cmd.arg(format!("--font-path={}", p.as_path().display()));
    }

    log::info!("running trace program: {:?}", cmd);

    let output = cmd.output().expect("trace program command failed to start");
    let stdout = output.stdout;
    let stderr = output.stderr;

    log::info!("trace program executed");

    let mut input_chan = std::io::Cursor::new(stdout);
    let messages = std::iter::from_fn(|| {
        if input_chan.position() == input_chan.get_ref().len() as u64 {
            return None;
        }
        let msg = lsp_server::Message::read(&mut input_chan).ok()?;
        Some(msg)
    })
    .flatten()
    .collect::<Vec<_>>();

    let stderr = base64::engine::general_purpose::STANDARD.encode(stderr);

    Ok(TraceReport {
        request: req,
        messages,
        stderr,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TraceReport {
    request: UserActionTraceRequest,
    messages: Vec<lsp_server::Message>,
    stderr: String,
}
