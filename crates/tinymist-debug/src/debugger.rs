//! Tinymist breakpoint support for Typst.

mod instr;

use std::sync::Arc;

use comemo::Tracked;
use parking_lot::RwLock;
use tinymist_std::hash::{FxHashMap, FxHashSet};
use tinymist_world::vfs::FileId;
use typst::diag::FileResult;
use typst::engine::Engine;
use typst::foundations::{func, Context};
use typst::syntax::{Source, Span};

use crate::instrument::Instrumenter;

#[derive(Default)]
pub struct BreakpointInstr {}

/// The kind of breakpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BreakpointKind {
    // Expr,
    // Line,
    /// A call breakpoint.
    CallStart,
    /// A call breakpoint.
    CallEnd,
    /// A function breakpoint.
    Function,
    /// A break breakpoint.
    Break,
    /// A continue breakpoint.
    Continue,
    /// A return breakpoint.
    Return,
    /// A block start breakpoint.
    BlockStart,
    /// A block end breakpoint.
    BlockEnd,
    /// A show start breakpoint.
    ShowStart,
    /// A show end breakpoint.
    ShowEnd,
    /// A doc start breakpoint.
    DocStart,
    /// A doc end breakpoint.
    DocEnd,
}

impl BreakpointKind {
    pub fn to_str(self) -> &'static str {
        match self {
            BreakpointKind::CallStart => "call_start",
            BreakpointKind::CallEnd => "call_end",
            BreakpointKind::Function => "function",
            BreakpointKind::Break => "break",
            BreakpointKind::Continue => "continue",
            BreakpointKind::Return => "return",
            BreakpointKind::BlockStart => "block_start",
            BreakpointKind::BlockEnd => "block_end",
            BreakpointKind::ShowStart => "show_start",
            BreakpointKind::ShowEnd => "show_end",
            BreakpointKind::DocStart => "doc_start",
            BreakpointKind::DocEnd => "doc_end",
        }
    }
}

#[derive(Default)]
pub struct BreakpointInfo {
    pub meta: Vec<BreakpointItem>,
}

pub struct BreakpointItem {
    pub origin_span: Span,
    pub kind: BreakpointKind,
}

static DEBUG_SESSION: RwLock<Option<DebugSession>> = RwLock::new(None);

pub trait DebugSessionHandler: Send + Sync {
    fn on_breakpoint(
        &self,
        engine: &Engine,
        context: Tracked<Context>,
        span: Span,
        kind: BreakpointKind,
    );
}

pub struct DebugSession {
    pub enabled: FxHashSet<(FileId, usize, BreakpointKind)>,
    /// The breakpoint meta.
    pub breakpoints: FxHashMap<FileId, Arc<BreakpointInfo>>,

    pub handler: Box<dyn DebugSessionHandler>,
}

/// Sets the debug session.
pub fn set_debug_session(session: Option<DebugSession>) -> bool {
    let mut lock = DEBUG_SESSION.write();

    if session.is_some() {
        return false;
    }

    let _ = std::mem::replace(&mut *lock, session);
    true
}

/// Software breakpoints
fn soft_breakpoint(
    engine: &Engine,
    context: Tracked<Context>,
    span: Span,
    id: usize,
    kind: BreakpointKind,
) -> Option<()> {
    let fid = span.id()?;

    let session = DEBUG_SESSION.read();
    let session = session.as_ref()?;

    let bp_feature = (fid, id, kind);
    if !session.enabled.contains(&bp_feature) {
        return None;
    }

    let item = session.breakpoints.get(&fid)?.meta.get(id)?;
    session
        .handler
        .on_breakpoint(engine, context, item.origin_span, kind);

    Some(())
}

pub mod breakpoints {

    use super::*;

    #[func(
        name = "__breakpoint_call_start",
        title = "A Software Breakpoint at the start of a call."
    )]
    pub fn __breakpoint_call_start(
        engine: &Engine,
        context: Tracked<Context>,
        span: Span,
        id: usize,
    ) {
        soft_breakpoint(engine, context, span, id, BreakpointKind::CallStart);
    }

    #[func(
        name = "__breakpoint_call_end",
        title = "A Software Breakpoint at the end of a call."
    )]
    pub fn __breakpoint_call_end(
        engine: &Engine,
        context: Tracked<Context>,
        span: Span,
        id: usize,
    ) {
        soft_breakpoint(engine, context, span, id, BreakpointKind::CallEnd);
    }

    #[func(
        name = "__breakpoint_function",
        title = "A Software Breakpoint at the start of a function."
    )]
    pub fn __breakpoint_function(
        engine: &Engine,
        context: Tracked<Context>,
        span: Span,
        id: usize,
    ) {
        soft_breakpoint(engine, context, span, id, BreakpointKind::Function);
    }

    #[func(
        name = "__breakpoint_break",
        title = "A Software Breakpoint at a break."
    )]
    pub fn __breakpoint_break(engine: &Engine, context: Tracked<Context>, span: Span, id: usize) {
        soft_breakpoint(engine, context, span, id, BreakpointKind::Break);
    }

    #[func(
        name = "__breakpoint_continue",
        title = "A Software Breakpoint at a continue."
    )]
    pub fn __breakpoint_continue(
        engine: &Engine,
        context: Tracked<Context>,
        span: Span,
        id: usize,
    ) {
        soft_breakpoint(engine, context, span, id, BreakpointKind::Continue);
    }

    #[func(
        name = "__breakpoint_return",
        title = "A Software Breakpoint at a return."
    )]
    pub fn __breakpoint_return(engine: &Engine, context: Tracked<Context>, span: Span, id: usize) {
        soft_breakpoint(engine, context, span, id, BreakpointKind::Return);
    }

    #[func(
        name = "__breakpoint_block_start",
        title = "A Software Breakpoint at the start of a block."
    )]
    pub fn __breakpoint_block_start(
        engine: &Engine,
        context: Tracked<Context>,
        span: Span,
        id: usize,
    ) {
        soft_breakpoint(engine, context, span, id, BreakpointKind::BlockStart);
    }

    #[func(
        name = "__breakpoint_block_end",
        title = "A Software Breakpoint at the end of a block."
    )]
    pub fn __breakpoint_block_end(
        engine: &Engine,
        context: Tracked<Context>,
        span: Span,
        id: usize,
    ) {
        soft_breakpoint(engine, context, span, id, BreakpointKind::BlockEnd);
    }

    #[func(
        name = "__breakpoint_show_start",
        title = "A Software Breakpoint at the start of a show."
    )]
    pub fn __breakpoint_show_start(
        engine: &Engine,
        context: Tracked<Context>,
        span: Span,
        id: usize,
    ) {
        soft_breakpoint(engine, context, span, id, BreakpointKind::ShowStart);
    }

    #[func(
        name = "__breakpoint_show_end",
        title = "A Software Breakpoint at the end of a show."
    )]
    pub fn __breakpoint_show_end(
        engine: &Engine,
        context: Tracked<Context>,
        span: Span,
        id: usize,
    ) {
        soft_breakpoint(engine, context, span, id, BreakpointKind::ShowEnd);
    }

    #[func(
        name = "__breakpoint_doc_start",
        title = "A Software Breakpoint at the start of a doc."
    )]
    pub fn __breakpoint_doc_start(
        engine: &Engine,
        context: Tracked<Context>,
        span: Span,
        id: usize,
    ) {
        soft_breakpoint(engine, context, span, id, BreakpointKind::DocStart);
    }

    #[func(
        name = "__breakpoint_doc_end",
        title = "A Software Breakpoint at the end of a doc."
    )]
    pub fn __breakpoint_doc_end(engine: &Engine, context: Tracked<Context>, span: Span, id: usize) {
        soft_breakpoint(engine, context, span, id, BreakpointKind::DocEnd);
    }
}
