use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use dapts::{BreakpointReason, ProcessEventStartMethod, ThreadEventReason};
use lsp_types::Url;
use reflexo::ImmutPath;
use reflexo_typst::{EntryReader, TaskInputs};
use serde::Deserialize;
use sync_ls::{
    internal_error, invalid_request, just_ok, RequestId, SchedulableResponse, ScheduledResult,
};
use tinymist_dap::DebugRequest;
use tinymist_std::error::prelude::*;
use typst::{World, WorldExt};

use super::*;

impl ServerState {
    /// Called at the end of the configuration sequence.
    /// Indicates that all breakpoints etc. have been sent to the DA and that
    /// the 'launch' can start.
    pub(crate) fn configuration_done(
        &mut self,
        _args: dapts::ConfigurationDoneArguments,
    ) -> SchedulableResponse<()> {
        just_ok(())
    }

    /// Resumes the debuggee after a stopped event.
    pub(crate) fn continue_debug(
        &mut self,
        args: dapts::ContinueArguments,
    ) -> SchedulableResponse<dapts::ContinueResponse> {
        let session = self.debug.session()?;
        if args.thread_id != session.adaptor.thread_id {
            return Err(invalid_request(format!(
                "unknown debug thread: {}",
                args.thread_id
            )));
        }

        session
            .adaptor
            .tx
            .send(DebugRequest::Continue)
            .map_err(|_| internal_error("debug session is closed"))?;

        just_ok(dapts::ContinueResponse {
            all_threads_continued: Some(true),
        })
    }

    /// Should stop the debug session.
    pub(crate) fn disconnect(
        &mut self,
        _args: dapts::DisconnectArguments,
    ) -> SchedulableResponse<()> {
        let _ = self.debug.session.take();

        just_ok(())
    }

    pub(crate) fn terminate_debug(
        &mut self,
        _args: dapts::TerminateArguments,
    ) -> SchedulableResponse<()> {
        let _ = self.debug.session.take();

        self.client
            .send_dap_event::<dapts::event::Terminated>(dapts::TerminatedEvent { restart: None });

        just_ok(())
    }

    pub(crate) fn terminate_debug_thread(
        &mut self,
        args: dapts::TerminateThreadsArguments,
    ) -> SchedulableResponse<()> {
        if args.thread_ids.as_ref().is_none_or(|id| id.is_empty()) {
            return just_ok(());
        }
        let terminate_thread_ok = args.thread_ids.into_iter().flatten().all(|id| id == 1);
        if terminate_thread_ok {
            let _ = self.debug.session.take();
        }

        just_ok(())
    }

    // cancelRequest

    pub(crate) fn attach_debug(
        &mut self,
        args: dapts::AttachRequestArguments,
    ) -> SchedulableResponse<()> {
        self.launch_debug_(
            dapts::LaunchRequestArguments { raw: args.raw },
            ProcessEventStartMethod::Attach,
        )
    }

    pub(crate) fn launch_debug(
        &mut self,
        args: dapts::LaunchRequestArguments,
    ) -> SchedulableResponse<()> {
        self.launch_debug_(args, ProcessEventStartMethod::Launch)
    }

    pub(crate) fn launch_debug_(
        &mut self,
        args: dapts::LaunchRequestArguments,
        method: ProcessEventStartMethod,
    ) -> SchedulableResponse<()> {
        // wait 1 second until configuration has finished (and configurationDoneRequest
        // has been called) await this._configurationDone.wait(1000);

        // start the program in the runtime
        let args = serde_json::from_value::<LaunchDebugArguments>(args.raw).unwrap();

        let program: ImmutPath = Path::new(&args.program).into();
        let root = Path::new(&args.root).into();
        let input = self.resolve_task(program.clone());
        let entry = self
            .entry_resolver()
            .resolve_with_root(Some(root), Some(program));

        // todo: respect lock file
        let input = TaskInputs {
            entry: Some(entry),
            inputs: input.inputs,
        };

        let snapshot = self.project.snapshot().unwrap().snap.clone().task(input);
        let world = &snapshot.world;

        let main = world
            .main_id()
            .ok_or_else(|| internal_error("No main file found"))?;
        let main_source = world.source(main).map_err(invalid_request)?;
        let main_eof = main_source.text().len();
        let source = main_source.clone();

        let (adaptor_tx, adaptor_rx) = std::sync::mpsc::channel();
        let adaptor = Arc::new(Debuggee {
            tx: adaptor_tx,
            current_span: Default::default(),
            stop_on_entry: args.stop_on_entry.unwrap_or_default(),
            thread_id: 1,
            client: self.client.clone().to_untyped(),
        });

        tinymist_dap::start_session(
            snapshot.world.clone(),
            adaptor.clone(),
            adaptor_rx,
            self.debug.function_breakpoints.clone(),
            self.debug
                .source_breakpoints
                .iter()
                .filter_map(|(path, breakpoints)| {
                    Some((
                        snapshot.world.file_id_by_path(path).ok()?,
                        breakpoints.clone(),
                    ))
                })
                .collect(),
        );

        self.debug.session = Some(DebugSession {
            config: self.config.const_dap_config.clone(),
            adaptor,
            snapshot,
            // Keep the main source and EOF position as the fallback stack frame.
            source,
            position: main_eof,
        });

        self.client
            .send_dap_event::<dapts::event::Process>(dapts::ProcessEvent {
                name: "typst".into(),
                start_method: Some(method),
                ..dapts::ProcessEvent::default()
            });

        self.client
            .send_dap_event::<dapts::event::Thread>(dapts::ThreadEvent {
                reason: ThreadEventReason::Started,
                thread_id: self.debug.session()?.adaptor.thread_id,
            });

        just_ok(())
    }

    // customRequest
}

impl ServerState {
    pub(crate) fn set_breakpoints(
        &mut self,
        args: dapts::SetBreakpointsArguments,
    ) -> SchedulableResponse<dapts::SetBreakpointsResponse> {
        let requested = source_breakpoints_from_args(args.breakpoints, args.lines);
        let Some(path) = self.dap_source_path(&args.source) else {
            return just_ok(dapts::SetBreakpointsResponse {
                breakpoints: requested
                    .iter()
                    .enumerate()
                    .map(|(idx, breakpoint)| {
                        failed_source_breakpoint(
                            idx,
                            &args.source,
                            breakpoint.line,
                            breakpoint.column,
                            "source breakpoints require source.path",
                        )
                    })
                    .collect(),
            });
        };

        let mut unsupported = Vec::with_capacity(requested.len());
        let mut source_breakpoints = Vec::new();
        for breakpoint in &requested {
            let unsupported_message = unsupported_source_breakpoint_message(breakpoint);
            unsupported.push(unsupported_message);

            if unsupported_message.is_none() {
                source_breakpoints.push(tinymist_debug::SourceBreakpoint {
                    line: self.dap_line_to_zero_based(breakpoint.line),
                    column: breakpoint
                        .column
                        .map(|column| self.dap_column_to_zero_based(column)),
                });
            }
        }

        if source_breakpoints.is_empty() {
            self.debug.source_breakpoints.remove(&path);
        } else {
            self.debug
                .source_breakpoints
                .insert(path.clone(), source_breakpoints.clone());
        }

        let active_resolutions = self
            .debug
            .session
            .as_ref()
            .and_then(|session| session.snapshot.world.source_by_path(&path).ok())
            .and_then(|source| {
                tinymist_debug::set_debug_source_breakpoints(source, source_breakpoints.clone())
            });
        let mut active_resolutions = active_resolutions.into_iter().flatten();

        let breakpoints = requested
            .iter()
            .zip(unsupported)
            .enumerate()
            .map(|(idx, (breakpoint, unsupported_message))| {
                if let Some(message) = unsupported_message {
                    return failed_source_breakpoint(
                        idx,
                        &args.source,
                        breakpoint.line,
                        breakpoint.column,
                        message,
                    );
                }

                let Some(resolution) = active_resolutions.next() else {
                    return pending_source_breakpoint(idx, &args.source, breakpoint);
                };

                let Some(resolved) = resolution.resolved else {
                    if resolution.pending {
                        return pending_source_breakpoint(idx, &args.source, breakpoint);
                    }

                    return failed_source_breakpoint(
                        idx,
                        &args.source,
                        breakpoint.line,
                        breakpoint.column,
                        "no block/function breakpoint location found near this line",
                    );
                };

                dapts::Breakpoint {
                    id: Some((idx + 1) as u64),
                    line: Some(self.zero_based_line_to_dap(resolved.line)),
                    column: Some(self.zero_based_column_to_dap(resolved.column)),
                    message: Some(format!(
                        "Mapped to nearest Typst {} breakpoint",
                        resolved.kind.to_str()
                    )),
                    source: Some(args.source.clone()),
                    verified: true,
                    ..dapts::Breakpoint::default()
                }
            })
            .collect();

        just_ok(dapts::SetBreakpointsResponse { breakpoints })
    }

    pub(crate) fn set_function_breakpoints(
        &mut self,
        args: dapts::SetFunctionBreakpointsArguments,
    ) -> SchedulableResponse<dapts::SetFunctionBreakpointsResponse> {
        self.debug.function_breakpoints = args
            .breakpoints
            .iter()
            .map(|breakpoint| breakpoint.name.clone())
            .collect();

        tinymist_debug::set_debug_function_breakpoints(self.debug.function_breakpoints.clone());

        just_ok(dapts::SetFunctionBreakpointsResponse {
            breakpoints: args
                .breakpoints
                .iter()
                .enumerate()
                .map(|(idx, breakpoint)| dapts::Breakpoint {
                    id: Some((idx + 1) as u64),
                    message: Some(format!("Function breakpoint: {}", breakpoint.name)),
                    verified: true,
                    ..dapts::Breakpoint::default()
                })
                .collect(),
        })
    }
}

impl ServerState {
    fn dap_source_path(&self, source: &dapts::Source) -> Option<PathBuf> {
        let path = source.path.as_ref()?;
        match self.config.const_dap_config.path_format {
            crate::DapPathFormat::Uri => {
                let uri = Url::parse(path).ok()?;
                Some(tinymist_query::url_to_path(&uri))
            }
            crate::DapPathFormat::Path | crate::DapPathFormat::Unknown => Some(PathBuf::from(path)),
            _ => Some(PathBuf::from(path)),
        }
    }

    fn dap_line_to_zero_based(&self, line: u32) -> u32 {
        if self.config.const_dap_config.lines_start_at1 {
            line.saturating_sub(1)
        } else {
            line
        }
    }

    fn dap_column_to_zero_based(&self, column: u32) -> u32 {
        if self.config.const_dap_config.columns_start_at1 {
            column.saturating_sub(1)
        } else {
            column
        }
    }

    fn zero_based_line_to_dap(&self, line: u32) -> u32 {
        if self.config.const_dap_config.lines_start_at1 {
            line.saturating_add(1)
        } else {
            line
        }
    }

    fn zero_based_column_to_dap(&self, column: u32) -> u32 {
        if self.config.const_dap_config.columns_start_at1 {
            column.saturating_add(1)
        } else {
            column
        }
    }
}

fn source_breakpoints_from_args(
    breakpoints: Option<Vec<dapts::SourceBreakpoint>>,
    lines: Option<Vec<u64>>,
) -> Vec<dapts::SourceBreakpoint> {
    if let Some(breakpoints) = breakpoints {
        return breakpoints;
    }

    lines
        .unwrap_or_default()
        .into_iter()
        .map(|line| dapts::SourceBreakpoint {
            column: None,
            condition: None,
            hit_condition: None,
            line: line.min(u32::MAX as u64) as u32,
            log_message: None,
            mode: None,
        })
        .collect()
}

fn unsupported_source_breakpoint_message(
    breakpoint: &dapts::SourceBreakpoint,
) -> Option<&'static str> {
    if breakpoint.condition.is_some() {
        return Some("conditional source breakpoints are not supported");
    }
    if breakpoint.hit_condition.is_some() {
        return Some("hit-count source breakpoints are not supported");
    }
    if breakpoint.log_message.is_some() {
        return Some("logpoints are not supported");
    }
    if breakpoint.mode.is_some() {
        return Some("breakpoint modes are not supported");
    }

    None
}

fn pending_source_breakpoint(
    idx: usize,
    source: &dapts::Source,
    breakpoint: &dapts::SourceBreakpoint,
) -> dapts::Breakpoint {
    dapts::Breakpoint {
        id: Some((idx + 1) as u64),
        line: Some(breakpoint.line),
        column: breakpoint.column,
        message: Some("Line breakpoint will bind to the nearest Typst block/function hook".into()),
        source: Some(source.clone()),
        verified: true,
        ..dapts::Breakpoint::default()
    }
}

fn failed_source_breakpoint(
    idx: usize,
    source: &dapts::Source,
    line: u32,
    column: Option<u32>,
    message: impl Into<String>,
) -> dapts::Breakpoint {
    dapts::Breakpoint {
        id: Some((idx + 1) as u64),
        line: Some(line),
        column,
        message: Some(message.into()),
        reason: Some(BreakpointReason::Failed),
        source: Some(source.clone()),
        verified: false,
        ..dapts::Breakpoint::default()
    }
}

/// This interface describes the mock-debug specific launch attributes
/// (which are not part of the Debug Adapter Protocol).
/// The schema for these attributes lives in the package.json of the mock-debug
/// extension. The interface should always match this schema.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchDebugArguments {
    /// An absolute path to the "program" to debug.
    program: String,
    /// The root directory of the program (used to resolve absolute paths).
    root: String,
    /// Automatically stop target after launch. If not specified, target does
    /// not stop.
    stop_on_entry: Option<bool>,
}

impl ServerState {
    pub(crate) fn debug_threads(
        &mut self,
        _args: (),
    ) -> SchedulableResponse<dapts::ThreadsResponse> {
        just_ok(dapts::ThreadsResponse {
            threads: vec![dapts::Thread {
                id: 1,
                name: "thread 1".into(),
            }],
        })
    }

    pub(crate) fn debug_stack_trace(
        &mut self,
        args: dapts::StackTraceArguments,
    ) -> SchedulableResponse<dapts::StackTraceResponse> {
        let session = self.debug.session()?;
        if args.thread_id != session.adaptor.thread_id {
            return Err(invalid_request(format!(
                "unknown debug thread: {}",
                args.thread_id
            )));
        }

        let current_span = *session.adaptor.current_span.lock();
        let current_location = current_span.and_then(|span| {
            let source = session.snapshot.world.source(span.id()?).ok()?;
            Some((session.snapshot.world.range(span)?, source))
        });
        let (source, position, line_count) = if let Some((range, source)) = current_location {
            (
                session.to_dap_source(source.id()),
                session.to_dap_position(range.start, &source),
                source.text().lines().count().max(1) as u64,
            )
        } else {
            (
                session.to_dap_source(session.source.id()),
                session.to_dap_position(session.position, &session.source),
                session.source.text().lines().count().max(1) as u64,
            )
        };
        let line = if session.config.lines_start_at1 {
            position.line.clamp(1, line_count)
        } else {
            position.line.min(line_count.saturating_sub(1))
        };

        just_ok(dapts::StackTraceResponse {
            stack_frames: vec![dapts::StackFrame {
                can_restart: None,
                column: position.character.min(u32::MAX as u64) as u32,
                end_column: None,
                end_line: None,
                id: 1,
                instruction_pointer_reference: None,
                line: line.min(u32::MAX as u64) as u32,
                module_id: None,
                name: "main".into(),
                presentation_hint: None,
                source: Some(source),
            }],
            total_frames: Some(1),
        })
    }
}

impl ServerState {
    pub(crate) fn evaluate_repl(
        &mut self,
        req_id: RequestId,
        args: dapts::EvaluateArguments,
    ) -> ScheduledResult {
        let session = self.debug.session()?;

        session
            .adaptor
            .tx
            .send(DebugRequest::Evaluate(
                RequestId::dap(req_id),
                args.expression,
            ))
            .map_err(|_| internal_error("debug session is closed"))?;
        Ok(Some(()))
    }

    pub(crate) fn complete_repl(
        &mut self,
        args: dapts::CompletionsArguments,
    ) -> SchedulableResponse<dapts::CompletionsResponse> {
        let _ = args;
        let session = self
            .debug
            .session
            .as_ref()
            .ok_or_else(|| internal_error("No debug session found"))?;

        just_ok(dapts::CompletionsResponse { targets: vec![] })
    }
}
