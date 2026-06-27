#![allow(unused)]

mod event;
mod init;
mod request;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use dapts::StoppedEventReason;
pub use init::*;
use parking_lot::Mutex;

use reflexo_typst::vfs::PathResolution;
use reflexo_typst::TypstPagedDocument;
use serde::{Deserialize, Serialize};
use sync_ls::{invalid_request, LspClient, LspResult};
use tinymist_dap::{
    BreakpointContext, DebugAdaptor, DebugRequest, DebugResult, DebugScope,
    DebugScopePresentationHint, DebugScopesResponse, DebugStackFrame, DebugStackTraceResponse,
    DebugVariable, DebugVariablesResponse,
};
use tinymist_query::PositionEncoding;
use typst::diag::{SourceResult, Warned};
use typst::foundations::{Repr, Value};
// use sync_lsp::RequestId;
use typst::syntax::{FileId, Source, Span};
use typst::{World, WorldExt};

use crate::project::LspCompileSnapshot;
use crate::{ConstDapConfig, ServerState};

#[derive(Default)]
pub(crate) struct DebugState {
    pub(crate) session: Option<DebugSession>,
    pub(crate) function_breakpoints: Vec<String>,
    pub(crate) source_breakpoints: HashMap<PathBuf, Vec<tinymist_debug::SourceBreakpoint>>,
}

impl DebugState {
    pub(crate) fn session(&self) -> LspResult<&DebugSession> {
        self.session
            .as_ref()
            .ok_or_else(|| invalid_request("No active debug session"))
    }
}

pub(crate) struct DebugSession {
    config: ConstDapConfig,
    adaptor: Arc<Debuggee>,

    snapshot: LspCompileSnapshot,

    /// The current source file.
    source: Source,
    /// The current position.
    position: usize,
}
// private _variableHandles = new Handles<"locals" | "globals" |
// RuntimeVariable>();

//     private _valuesInHex = false;
//     private _useInvalidatedEvent = false;

const DAP_POS_ENCODING: PositionEncoding = PositionEncoding::Utf16;

impl DebugSession {
    pub fn to_dap_source(&self, id: FileId) -> dapts::Source {
        use dapts::Source;
        Source {
            path: match self.snapshot.world.path_for_id(id).ok() {
                Some(PathResolution::Resolved(path)) => Some(path.display().to_string()),
                None | Some(PathResolution::Rootless(..)) => None,
            },
            ..Source::default()
        }
    }

    pub fn to_dap_position(&self, pos: usize, source: &Source) -> DapPosition {
        let mut lsp_pos = tinymist_query::to_lsp_position(pos, DAP_POS_ENCODING, source);

        if self.config.lines_start_at1 {
            lsp_pos.line += 1;
        }
        if self.config.columns_start_at1 {
            lsp_pos.character += 1;
        }

        DapPosition {
            line: lsp_pos.line as u64,
            character: lsp_pos.character as u64,
        }
    }
}

/// Position in a text document expressed as line and character offset.
/// A position is between two characters like an 'insert' cursor in a editor.
///
/// Whether or not the line and column are 0 or 1-based is negotiated between
/// the client and server.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Default, Deserialize, Serialize)]
pub struct DapPosition {
    /// Line position in a document.
    pub line: u64,
    /// Character offset on a line in a document.
    ///
    /// If the character value is greater than the line length it defaults back
    /// to the line length.
    pub character: u64,
}

struct Debuggee {
    tx: std::sync::mpsc::Sender<DebugRequest>,
    config: ConstDapConfig,
    snapshot: LspCompileSnapshot,
    /// The main source file used for detached start/end frames.
    source: Source,
    /// The fallback position used for document-end frames.
    position: usize,
    /// Whether the debugger should stop on entry.
    stop_on_entry: bool,
    /// A faked thread id. We don't support multiple threads, so we can use a
    /// hardcoded ID for the default thread.
    thread_id: u64,
    /// The client to respond DAP messages.
    client: LspClient,
}

impl Debuggee {
    fn respond_dap<T: Serialize>(&self, id: i64, result: DebugResult<T>) {
        let req_id = sync_ls::RequestId::from(id as i32);
        let response = match result {
            Ok(body) => sync_ls::dap::Response::success(id, body),
            Err(err) => sync_ls::dap::Response::error(id, Some(err), None),
        };

        self.client.respond(req_id, response.into());
    }

    fn to_dap_source(&self, id: FileId) -> dapts::Source {
        use dapts::Source;
        Source {
            path: match self.snapshot.world.path_for_id(id).ok() {
                Some(PathResolution::Resolved(path)) => Some(path.display().to_string()),
                None | Some(PathResolution::Rootless(..)) => None,
            },
            ..Source::default()
        }
    }

    fn to_dap_position(&self, pos: usize, source: &Source) -> DapPosition {
        let mut lsp_pos = tinymist_query::to_lsp_position(pos, DAP_POS_ENCODING, source);

        if self.config.lines_start_at1 {
            lsp_pos.line += 1;
        }
        if self.config.columns_start_at1 {
            lsp_pos.character += 1;
        }

        DapPosition {
            line: lsp_pos.line as u64,
            character: lsp_pos.character as u64,
        }
    }

    fn fallback_position(&self, kind: tinymist_dap::BreakpointKind) -> usize {
        if matches!(kind, tinymist_dap::BreakpointKind::BeforeCompile) {
            0
        } else {
            self.position
        }
    }

    fn frame_location(
        &self,
        span: Span,
        kind: tinymist_dap::BreakpointKind,
    ) -> (dapts::Source, DapPosition, Option<DapPosition>, u64) {
        if let Some((source, start, end, line_count)) = self.span_location(span) {
            return (source, start, Some(end), line_count);
        }

        let position = self.to_dap_position(self.fallback_position(kind), &self.source);
        (
            self.to_dap_source(self.source.id()),
            position,
            None,
            self.source.text().lines().count().max(1) as u64,
        )
    }

    fn scope_location(&self, span: Span) -> Option<(dapts::Source, DapPosition, DapPosition, u64)> {
        self.span_location(span)
    }

    fn span_location(&self, span: Span) -> Option<(dapts::Source, DapPosition, DapPosition, u64)> {
        let source = self.snapshot.world.source(span.id()?).ok()?;
        let range = self.snapshot.world.range(span)?;
        let start = self.to_dap_position(range.start, &source);
        let end = self.to_dap_position(range.end, &source);
        let line_count = source.text().lines().count().max(1) as u64;

        Some((self.to_dap_source(source.id()), start, end, line_count))
    }

    fn clamp_line(&self, line: u64, line_count: u64) -> u32 {
        let line = if self.config.lines_start_at1 {
            line.clamp(1, line_count)
        } else {
            line.min(line_count.saturating_sub(1))
        };

        line.min(u32::MAX as u64) as u32
    }

    fn position_column(position: DapPosition) -> u32 {
        position.character.min(u32::MAX as u64) as u32
    }

    fn stack_frame(&self, frame: DebugStackFrame) -> dapts::StackFrame {
        let (source, start, end, line_count) = self.frame_location(frame.span, frame.kind);

        dapts::StackFrame {
            can_restart: Some(false),
            column: Self::position_column(start),
            end_column: end.map(Self::position_column),
            end_line: end.map(|end| self.clamp_line(end.line, line_count)),
            id: frame.id,
            instruction_pointer_reference: None,
            line: self.clamp_line(start.line, line_count),
            module_id: None,
            name: frame.name,
            presentation_hint: Some(if frame.span.id().is_some() {
                dapts::StackFramePresentationHint::Normal
            } else {
                dapts::StackFramePresentationHint::Subtle
            }),
            source: Some(source),
        }
    }

    fn scope(&self, scope: DebugScope) -> dapts::Scope {
        let location = self.scope_location(scope.span);
        let (source, start, end, line_count) = match location {
            Some(location) => {
                let (source, start, end, line_count) = location;
                (Some(source), Some(start), Some(end), Some(line_count))
            }
            None => (None, None, None, None),
        };

        dapts::Scope {
            column: start.map(Self::position_column),
            end_column: end.map(Self::position_column),
            end_line: end
                .zip(line_count)
                .map(|(end, line_count)| self.clamp_line(end.line, line_count)),
            expensive: scope.expensive,
            indexed_variables: scope.indexed_variables,
            line: start
                .zip(line_count)
                .map(|(start, line_count)| self.clamp_line(start.line, line_count)),
            name: scope.name,
            named_variables: scope.named_variables,
            presentation_hint: scope.presentation_hint.map(|hint| match hint {
                DebugScopePresentationHint::Arguments => dapts::ScopePresentationHint::Arguments,
                DebugScopePresentationHint::Locals => dapts::ScopePresentationHint::Locals,
            }),
            source,
            variables_reference: scope.variables_reference,
        }
    }

    fn variable(&self, variable: DebugVariable) -> dapts::Variable {
        dapts::Variable {
            declaration_location_reference: None,
            evaluate_name: variable.evaluate_name,
            indexed_variables: variable.indexed_variables,
            memory_reference: None,
            name: variable.name,
            named_variables: variable.named_variables,
            presentation_hint: None,
            ty: variable.ty,
            value: variable.value,
            value_location_reference: None,
            variables_reference: variable.variables_reference,
        }
    }
}

impl DebugAdaptor for Debuggee {
    fn before_compile(&self) {
        if self.stop_on_entry {
            self.client
                .send_dap_event::<dapts::event::Stopped>(dapts::StoppedEvent {
                    all_threads_stopped: Some(true),
                    reason: StoppedEventReason::Entry,
                    description: Some("Paused at the start of the document".into()),
                    thread_id: Some(self.thread_id),
                    hit_breakpoint_ids: None,
                    preserve_focus_hint: Some(false),
                    text: None,
                });
        }
    }

    fn after_compile(&self, result: Warned<SourceResult<TypstPagedDocument>>) {
        // if args.compile_error {
        // simulate a compile/build error in "launch" request:
        // the error should not result in a modal dialog since 'showUser' is
        // set to false. A missing 'showUser' should result in a
        // modal dialog.

        // this.sendErrorResponse(response, {
        //   id: 1001,
        //   format: `compile error: some fake error.`,
        //   showUser:
        //     args.compileError === "show" ? true : args.compileError ===
        // "hide" ? false : undefined, });
        // } else {
        // this.sendResponse(response);
        // }

        // Stop at the end of compilation so the REPL can inspect module scope.
        self.client
            .send_dap_event::<dapts::event::Stopped>(dapts::StoppedEvent {
                all_threads_stopped: Some(true),
                reason: StoppedEventReason::Pause,
                description: Some("Paused at the end of the document".into()),
                thread_id: Some(self.thread_id),
                hit_breakpoint_ids: None,
                preserve_focus_hint: Some(false),
                text: None,
            });
    }

    fn terminate(&self) {}

    fn stopped(&self, ctx: &BreakpointContext) {
        let source_span = ctx.source_span();
        if source_span.id().is_none() {
            return;
        }

        let reason = match ctx.kind {
            tinymist_dap::BreakpointKind::Function => StoppedEventReason::FunctionBreakpoint,
            _ => StoppedEventReason::Breakpoint,
        };

        self.client
            .send_dap_event::<dapts::event::Stopped>(dapts::StoppedEvent {
                all_threads_stopped: Some(true),
                reason,
                description: Some(format!("Paused at {} breakpoint", ctx.kind.to_str())),
                thread_id: Some(self.thread_id),
                hit_breakpoint_ids: None,
                preserve_focus_hint: Some(false),
                text: None,
            });
    }

    fn respond_evaluate(&self, id: i64, result: SourceResult<Value>) {
        let req_id = sync_ls::RequestId::from(id as i32);
        let response = match result {
            Ok(val) => sync_ls::dap::Response::success(
                id,
                dapts::EvaluateResponse {
                    result: format!("{}", val.repr()),
                    ty: Some(format!("{}", val.ty().repr())),
                    ..dapts::EvaluateResponse::default()
                },
            ),
            Err(err) => sync_ls::dap::Response::error(id, Some(format!("{err:?}")), None),
        };

        self.client.respond(req_id, response.into());
    }

    fn respond_stack_trace(&self, id: i64, result: DebugResult<DebugStackTraceResponse>) {
        self.respond_dap(
            id,
            result.map(|response| dapts::StackTraceResponse {
                total_frames: Some(response.total_frames),
                stack_frames: response
                    .stack_frames
                    .into_iter()
                    .map(|frame| self.stack_frame(frame))
                    .collect(),
            }),
        );
    }

    fn respond_scopes(&self, id: i64, result: DebugResult<DebugScopesResponse>) {
        self.respond_dap(
            id,
            result.map(|response| dapts::ScopesResponse {
                scopes: response
                    .scopes
                    .into_iter()
                    .map(|scope| self.scope(scope))
                    .collect(),
            }),
        );
    }

    fn respond_variables(&self, id: i64, result: DebugResult<DebugVariablesResponse>) {
        self.respond_dap(
            id,
            result.map(|response| dapts::VariablesResponse {
                variables: response
                    .variables
                    .into_iter()
                    .map(|variable| self.variable(variable))
                    .collect(),
            }),
        );
    }
}
