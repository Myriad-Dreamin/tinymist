#![allow(unused)]

mod event;
mod init;
mod request;

use std::sync::Arc;

use dapts::StoppedEventReason;
pub use init::*;

use reflexo_typst::vfs::PathResolution;
use reflexo_typst::TypstPagedDocument;
use serde::{Deserialize, Serialize};
use sync_ls::{invalid_request, just_ok, LspClient, LspResult};
use tinymist_dap::{BreakpointContext, DebugAdaptor, DebugRequest};
use tinymist_query::PositionEncoding;
use typst::diag::{SourceResult, Warned};
use typst::foundations::{Repr, Value};
// use sync_lsp::RequestId;
use typst::syntax::{FileId, Source};

use crate::project::LspCompileSnapshot;
use crate::{ConstDapConfig, ServerState};

#[derive(Default)]
pub(crate) struct DebugState {
    pub(crate) session: Option<DebugSession>,
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
    adaptor: Arc<Debugee>,

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

struct Debugee {
    tx: std::sync::mpsc::Sender<DebugRequest>,
    /// Whether the debugger should stop on entry.
    stop_on_entry: bool,
    /// A faked thread id. We don't support multiple threads, so we can use a
    /// hardcoded ID for the default thread.
    thread_id: u64,
    /// The client to respond DAP messages.
    client: LspClient,
}

impl DebugAdaptor for Debugee {
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

        // Since we haven't implemented breakpoints, we can only stop intermediately and
        // response completions in repl console.
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

        // Creates a fake breakpoint at the end of document.
        // const MAIN_EOF: u64 = 1;
        // let source = self.debug.session()?.to_dap_source(main);
        // let pos = self
        //     .debug
        //     .session()?
        //     .to_dap_position(main_eof, &main_source);
        // let _ = self.debug.session()?.source;
        // let _ = self.debug.session()?.position;
        // self.client
        //     .send_dap_event::<dapts::event::Breakpoint>(dapts::BreakpointEvent {
        //         breakpoint: Breakpoint {
        //             message: Some("The end of the document".into()),
        //             source: Some(source),
        //             line: Some(pos.line),
        //             column: None,
        //             verified: false,
        //             end_column: None,
        //             end_line: None,
        //             offset: None,
        //             id: Some(MAIN_EOF),
        //             instruction_reference: None,
        //             reason: None,
        //         },
        //         reason: BreakpointEventReason::New,
        //     });
    }

    fn respond(&self, id: i64, result: SourceResult<Value>) {
        // todo: compile error
        let val = result.unwrap_or(Value::None);

        let req_id = sync_ls::RequestId::from(id as i32);

        self.client.schedule(
            req_id,
            just_ok(dapts::EvaluateResponse {
                result: format!("{}", val.repr()),
                ty: Some(format!("{}", val.ty().repr())),
                ..dapts::EvaluateResponse::default()
            }),
        );
    }
}
