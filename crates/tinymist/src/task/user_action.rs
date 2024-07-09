//! The actor that runs user actions.

use std::path::PathBuf;

use anyhow::bail;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sync_lsp::{just_future, SchedulableResponse};
use typst_ts_core::TypstDict;

use crate::internal_error;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceParams {
    pub compiler_program: PathBuf,
    pub root: PathBuf,
    pub main: PathBuf,
    pub inputs: TypstDict,
    pub font_paths: Vec<PathBuf>,
}

#[derive(Default, Clone, Copy)]
pub struct UserActionTask;

impl UserActionTask {
    pub fn trace(&self, params: TraceParams) -> SchedulableResponse<TraceReport> {
        just_future(async move {
            run_trace_program(params)
                .await
                .map_err(|e| internal_error(format!("failed to run trace program: {e:?}")))
        })
    }
}

/// Run a perf trace to some typst program
async fn run_trace_program(params: TraceParams) -> anyhow::Result<TraceReport> {
    // Typst compile root, input, font paths, inputs
    let mut cmd = tokio::process::Command::new(&params.compiler_program);
    let mut cmd = &mut cmd;

    cmd = cmd.arg("compile");

    cmd = cmd
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

    let output = cmd.output().await;
    let output = output.expect("trace program command failed to start");
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
pub struct TraceReport {
    request: TraceParams,
    messages: Vec<lsp_server::Message>,
    stderr: String,
}
