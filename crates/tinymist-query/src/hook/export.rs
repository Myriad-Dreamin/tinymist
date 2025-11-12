use std::time::Duration;

use anyhow::Context;
use ecow::eco_format;
use tinymist_project::{EntryReader, ExportTask, LspWorld};
use tinymist_std::error::prelude::*;
use typst::{
    diag::{At, SourceResult, StrResult},
    foundations::{Dict, Func, Str, Value},
    syntax::Span,
};

use crate::hook::HookScript;

/// The state desc of an export script.
pub enum ExportState {
    /// A debounce state.
    Debounce {
        /// The world to run the script in.
        world: LspWorld,
        /// The inner function to debounce.
        inner: Func,
        /// The duration to debounce.
        duration: Duration,
        /// The time the state was last checked.
        checked: tinymist_std::time::Time,
    },
    /// A finished state.
    Finished {
        /// The tasks to run.
        task: Vec<ExportTask>,
    },
}

/// Runs an export script.
pub fn run_export_script(world: &LspWorld, code: &str, inputs: Dict) -> Result<ExportState> {
    let result = super::eval_script(world, HookScript::Code(code), inputs, &world.entry_state())?;
    check_script_res(result)
}

/// Determines the export of a state.
pub fn determine_export(state: ExportState) -> Result<ExportState> {
    match state {
        ExportState::Debounce {
            world,
            inner,
            duration,
            checked,
        } => {
            let now = tinymist_std::time::now();
            if now
                .duration_since(checked)
                .context("failed to get duration since last checked")?
                < duration
            {
                Ok(ExportState::Debounce {
                    world,
                    inner,
                    duration,
                    checked,
                })
            } else {
                check_script_res(super::eval_script(
                    &world,
                    HookScript::Callback(inner),
                    Dict::default(),
                    &world.entry_state(),
                )?)
            }
        }
        ExportState::Finished { task } => Ok(ExportState::Finished { task }),
    }
}

fn check_script_res((world, res): (LspWorld, Value)) -> Result<ExportState> {
    match res {
        Value::Dict(d) => {
            let kind = match d.get("kind") {
                Ok(Value::Str(kind)) => kind,
                _ => bail!("expected result.kind to be a string"),
            };
            Ok(match kind.as_str() {
                "debounce" => {
                    let inner = match d.get("inner") {
                        Ok(Value::Func(func)) => func.clone(),
                        _ => bail!("expected result.inner to be a function"),
                    };
                    let duration = match d.get("duration") {
                        Ok(Value::Int(duration)) => Duration::from_millis((*duration) as u64),
                        _ => bail!("expected result.duration to be a duration"),
                    };
                    ExportState::Debounce {
                        world,
                        inner,
                        duration,
                        checked: tinymist_std::time::now(),
                    }
                }
                _ => bail!("expected result.kind to be 'debounce'"),
            })
        }
        _ => bail!("expected result to be a dictionary"),
    }
}

#[typst_macros::func(title = "debounce function")]
pub(crate) fn debounce(span: Span, duration: Str, inner: Func) -> SourceResult<Dict> {
    let duration = parse_time(duration.as_str()).at(span)?;
    let mut res = Dict::default();

    res.insert("inner".into(), Value::Func(inner.clone()));
    res.insert("kind".into(), Value::Str("debounce".into()));
    res.insert("duration".into(), Value::Int(duration.as_millis() as i64));

    // let global = engine.world.library().global.scope();
    // let sys = global.get("sys").unwrap().read().scope().unwrap();
    // let inputs = sys.get("inputs").unwrap().read().clone();
    // let last = match inputs {
    //     Value::Dict(dict) => dict.get("x-last").at(span)?.clone(),
    //     _ => return Err(eco_format!("expected sys.inputs to be a
    // dict")).at(span), };
    // let last_duration = match last {
    //     Value::Str(stamp) => Duration::from_millis(
    //         stamp
    //             .as_str()
    //             .parse::<u64>()
    //             .map_err(|e| eco_format!("expected sys.inputs.x-last to be a int,
    // but {e}"))             .at(span)?,
    //     ),
    //     _ => return Err(eco_format!("expected sys.inputs.x-last to be a
    // int")).at(span), };

    Ok(res)
}

fn parse_time(spec: &str) -> StrResult<Duration> {
    let (digits, unit) = if let Some(digits) = spec.strip_suffix("ms") {
        (digits, 1u64)
    } else if let Some(digits) = spec.strip_suffix("s") {
        (digits, 1000u64)
    } else {
        return Err("expected time spec like `5s` or `5ms`".into());
    };

    let digits = digits
        .parse::<u64>()
        .map_err(|e| eco_format!("expected time spec like `5s` or `5ms`, but {e}"))?;
    Ok(Duration::from_millis(digits * unit))
}
