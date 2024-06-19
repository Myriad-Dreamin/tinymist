//! The actor that runs user actions.

use std::path::PathBuf;

use anyhow::bail;
use base64::Engine;
use lsp_server::RequestId;
use serde::{Deserialize, Serialize};
use typst_ts_core::TypstDict;

use crate::{internal_error, result_to_response, LspHost, LanguageState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceParams {
    pub compiler_program: PathBuf,
    pub root: PathBuf,
    pub main: PathBuf,
    pub inputs: TypstDict,
    pub font_paths: Vec<PathBuf>,
}

pub enum UserActionRequest {
    Trace(RequestId, TraceParams),
}

pub fn run_user_action_thread(
    user_action_rx: crossbeam_channel::Receiver<UserActionRequest>,
    client: LspHost<LanguageState>,
) {
    while let Ok(req) = user_action_rx.recv() {
        match req {
            UserActionRequest::Trace(id, params) => {
                let res = run_trace_program(params)
                    .map_err(|e| internal_error(format!("failed to run trace program: {:?}", e)));

                client.respond(result_to_response(id, res));
            }
        }
    }

    log::info!("Trace thread did shut down");
}

/// Run a perf trace to some typst program
fn run_trace_program(params: TraceParams) -> anyhow::Result<TraceReport> {
    // Typst compile root, input, font paths, inputs
    let mut cmd = std::process::Command::new(&params.compiler_program);
    let mut cmd = &mut cmd;

    cmd = cmd.arg("compile");

    cmd = cmd
        .arg("--root")
        .arg(params.root.as_path())
        .arg(params.main.as_path());

    // todo: test space in input?
    for (k, v) in params.inputs.iter() {
        let typst::foundations::Value::Str(s) = v else {
            bail!("input value must be string, got {:?} for {:?}", v, k);
        };
        cmd = cmd.arg(format!("--input={k}={}", s.as_str()));
    }
    for p in &params.font_paths {
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
        request: params,
        messages,
        stderr,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TraceReport {
    request: TraceParams,
    messages: Vec<lsp_server::Message>,
    stderr: String,
}
