#![allow(unused)]

mod event;
mod init;
mod request;

pub use init::*;

use reflexo_typst::vfs::PathResolution;
use serde::{Deserialize, Serialize};
use sync_ls::{invalid_request, LspResult};
use tinymist_query::PositionEncoding;
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

    snapshot: LspCompileSnapshot,
    /// A faked thread id. We don't support multiple threads, so we can use a
    /// hardcoded ID for the default thread.
    thread_id: u64,
    /// Whether the debugger should stop on entry.
    stop_on_entry: bool,

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
