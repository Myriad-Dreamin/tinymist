//! Tinymist breakpoint support for Typst.

mod instr;

use std::sync::Arc;

use comemo::Tracked;
use parking_lot::RwLock;
use tinymist_std::hash::{FxHashMap, FxHashSet};
use tinymist_world::vfs::FileId;
use typst::diag::FileResult;
use typst::engine::Engine;
use typst::foundations::{func, Binding, Context, Dict, Scopes};
use typst::syntax::{Source, Span};
use typst::World;

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
    /// A before compile breakpoint.
    BeforeCompile,
    /// A after compile breakpoint.
    AfterCompile,
}

impl BreakpointKind {
    /// Converts the breakpoint kind to a string.
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
            BreakpointKind::BeforeCompile => "before_compile",
            BreakpointKind::AfterCompile => "after_compile",
        }
    }
}

#[derive(Default)]
pub struct BreakpointInfo {
    pub meta: Vec<BreakpointItem>,
}

pub struct BreakpointItem {
    pub origin_span: Span,
}

static DEBUG_SESSION: RwLock<Option<DebugSession>> = RwLock::new(None);

/// The debug session handler.
pub trait DebugSessionHandler: Send + Sync {
    /// Called when a breakpoint is hit.
    fn on_breakpoint(
        &self,
        engine: &Engine,
        context: Tracked<Context>,
        scopes: Scopes,
        span: Span,
        kind: BreakpointKind,
    );
}

/// The debug session.
pub struct DebugSession {
    enabled: FxHashSet<(FileId, usize, BreakpointKind)>,
    /// The breakpoint meta.
    breakpoints: FxHashMap<FileId, Arc<BreakpointInfo>>,

    /// The handler.
    pub handler: Arc<dyn DebugSessionHandler>,
}

impl DebugSession {
    /// Creates a new debug session.
    pub fn new(handler: Arc<dyn DebugSessionHandler>) -> Self {
        Self {
            enabled: FxHashSet::default(),
            breakpoints: FxHashMap::default(),
            handler,
        }
    }
}

/// Runs function with the debug session.
pub fn with_debug_session<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&DebugSession) -> R,
{
    Some(f(DEBUG_SESSION.read().as_ref()?))
}

/// Sets the debug session.
pub fn set_debug_session(session: Option<DebugSession>) -> bool {
    let mut lock = DEBUG_SESSION.write();

    if lock.is_some() {
        return false;
    }

    let _ = std::mem::replace(&mut *lock, session);
    true
}

/// Software breakpoints
fn check_soft_breakpoint(span: Span, id: usize, kind: BreakpointKind) -> Option<bool> {
    let fid = span.id()?;

    let session = DEBUG_SESSION.read();
    let session = session.as_ref()?;

    let bp_feature = (fid, id, kind);
    Some(session.enabled.contains(&bp_feature))
}

/// Software breakpoints
fn soft_breakpoint_handle(
    engine: &Engine,
    context: Tracked<Context>,
    span: Span,
    id: usize,
    kind: BreakpointKind,
    scope: Option<Dict>,
) -> Option<()> {
    let fid = span.id()?;

    let (handler, origin_span) = {
        let session = DEBUG_SESSION.read();
        let session = session.as_ref()?;

        let bp_feature = (fid, id, kind);
        if !session.enabled.contains(&bp_feature) {
            return None;
        }

        let item = session.breakpoints.get(&fid)?.meta.get(id)?;
        (session.handler.clone(), item.origin_span)
    };

    let mut scopes = Scopes::new(Some(engine.world.library()));
    if let Some(scope) = scope {
        for (key, value) in scope.into_iter() {
            scopes.top.bind(key.into(), Binding::detached(value));
        }
    }

    handler.on_breakpoint(engine, context, scopes, origin_span, kind);
    Some(())
}

pub mod breakpoints {

    use super::*;

    macro_rules! bp_handler {
        ($name:ident, $name2:expr, $name3:ident, $name4:expr, $title:expr, $kind:ident) => {
            #[func(name = $name2, title = $title)]
            pub fn $name(span: Span, id: usize) -> bool {
                check_soft_breakpoint(span, id, BreakpointKind::$kind).unwrap_or_default()
            }
            #[func(name = $name4, title = $title)]
            pub fn $name3(
                engine: &Engine,
                context: Tracked<Context>,
                span: Span,
                id: usize,
                scope: Option<Dict>,
            ) {
                soft_breakpoint_handle(engine, context, span, id, BreakpointKind::$kind, scope);
            }
        };
    }

    bp_handler!(
        __breakpoint_call_start,
        "__breakpoint_call_start",
        __breakpoint_call_start_handle,
        "__breakpoint_call_start_handle",
        "A Software Breakpoint at the start of a call.",
        CallStart
    );
    bp_handler!(
        __breakpoint_call_end,
        "__breakpoint_call_end",
        __breakpoint_call_end_handle,
        "__breakpoint_call_end_handle",
        "A Software Breakpoint at the end of a call.",
        CallEnd
    );
    bp_handler!(
        __breakpoint_function,
        "__breakpoint_function",
        __breakpoint_function_handle,
        "__breakpoint_function_handle",
        "A Software Breakpoint at the start of a function.",
        Function
    );
    bp_handler!(
        __breakpoint_break,
        "__breakpoint_break",
        __breakpoint_break_handle,
        "__breakpoint_break_handle",
        "A Software Breakpoint at a break.",
        Break
    );
    bp_handler!(
        __breakpoint_continue,
        "__breakpoint_continue",
        __breakpoint_continue_handle,
        "__breakpoint_continue_handle",
        "A Software Breakpoint at a continue.",
        Continue
    );
    bp_handler!(
        __breakpoint_return,
        "__breakpoint_return",
        __breakpoint_return_handle,
        "__breakpoint_return_handle",
        "A Software Breakpoint at a return.",
        Return
    );
    bp_handler!(
        __breakpoint_block_start,
        "__breakpoint_block_start",
        __breakpoint_block_start_handle,
        "__breakpoint_block_start_handle",
        "A Software Breakpoint at the start of a block.",
        BlockStart
    );
    bp_handler!(
        __breakpoint_block_end,
        "__breakpoint_block_end",
        __breakpoint_block_end_handle,
        "__breakpoint_block_end_handle",
        "A Software Breakpoint at the end of a block.",
        BlockEnd
    );
    bp_handler!(
        __breakpoint_show_start,
        "__breakpoint_show_start",
        __breakpoint_show_start_handle,
        "__breakpoint_show_start_handle",
        "A Software Breakpoint at the start of a show.",
        ShowStart
    );
    bp_handler!(
        __breakpoint_show_end,
        "__breakpoint_show_end",
        __breakpoint_show_end_handle,
        "__breakpoint_show_end_handle",
        "A Software Breakpoint at the end of a show.",
        ShowEnd
    );
    bp_handler!(
        __breakpoint_doc_start,
        "__breakpoint_doc_start",
        __breakpoint_doc_start_handle,
        "__breakpoint_doc_start_handle",
        "A Software Breakpoint at the start of a doc.",
        DocStart
    );
    bp_handler!(
        __breakpoint_doc_end,
        "__breakpoint_doc_end",
        __breakpoint_doc_end_handle,
        "__breakpoint_doc_end_handle",
        "A Software Breakpoint at the end of a doc.",
        DocEnd
    );
    bp_handler!(
        __breakpoint_before_compile,
        "__breakpoint_before_compile",
        __breakpoint_before_compile_handle,
        "__breakpoint_before_compile_handle",
        "A Software Breakpoint before compilation.",
        BeforeCompile
    );
    bp_handler!(
        __breakpoint_after_compile,
        "__breakpoint_after_compile",
        __breakpoint_after_compile_handle,
        "__breakpoint_after_compile_handle",
        "A Software Breakpoint after compilation.",
        AfterCompile
    );
}
