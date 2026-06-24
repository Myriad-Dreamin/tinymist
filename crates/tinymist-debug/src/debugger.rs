//! Tinymist breakpoint support for Typst.

mod instr;

use std::sync::Arc;

use comemo::Tracked;
use parking_lot::RwLock;
use tinymist_analysis::location::{PositionEncoding, to_lsp_position};
use tinymist_std::hash::{FxHashMap, FxHashSet};
use tinymist_world::vfs::FileId;
use typst::World;
use typst::diag::FileResult;
use typst::engine::Engine;
use typst::foundations::{Binding, Context, Dict, Scopes, func};
use typst::syntax::{Source, Span};
use typst_shim::syntax::source_range;

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
    pub kind: BreakpointKind,
    pub function_name: Option<String>,
    pub origin_span: Span,
}

/// A source breakpoint requested by line and optional column.
///
/// Lines and columns are zero-based. Columns are measured as UTF-16 code units.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceBreakpoint {
    /// The requested source line.
    pub line: u32,
    /// The requested source column.
    pub column: Option<u32>,
}

/// The result of resolving a source breakpoint to a structural breakpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceBreakpointResolution {
    /// The original requested breakpoint.
    pub requested: SourceBreakpoint,
    /// The structural breakpoint this request maps to, if any.
    pub resolved: Option<ResolvedSourceBreakpoint>,
    /// Whether the source breakpoint is waiting for instrumentation metadata.
    pub pending: bool,
}

/// A structural breakpoint selected for a source breakpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedSourceBreakpoint {
    /// The structural breakpoint id within the instrumented source metadata.
    pub id: usize,
    /// The kind of structural breakpoint.
    pub kind: BreakpointKind,
    /// The resolved zero-based source line.
    pub line: u32,
    /// The resolved zero-based UTF-16 source column.
    pub column: u32,
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
    enabled_function_breakpoints: FxHashSet<(FileId, usize, BreakpointKind)>,
    enabled_source_breakpoints: FxHashSet<(FileId, usize, BreakpointKind)>,
    function_breakpoints: FxHashSet<String>,
    source_breakpoints: FxHashMap<FileId, Vec<SourceBreakpoint>>,
    /// The breakpoint meta.
    breakpoints: FxHashMap<FileId, Arc<BreakpointInfo>>,

    /// The handler.
    pub handler: Arc<dyn DebugSessionHandler>,
}

impl DebugSession {
    /// Creates a new debug session.
    pub fn new(handler: Arc<dyn DebugSessionHandler>) -> Self {
        Self {
            enabled_function_breakpoints: FxHashSet::default(),
            enabled_source_breakpoints: FxHashSet::default(),
            function_breakpoints: FxHashSet::default(),
            source_breakpoints: FxHashMap::default(),
            breakpoints: FxHashMap::default(),
            handler,
        }
    }

    /// Replaces the currently enabled function breakpoints by name.
    pub fn set_function_breakpoints(&mut self, names: impl IntoIterator<Item = String>) {
        self.function_breakpoints = names.into_iter().collect();
        self.enabled_function_breakpoints.clear();

        for (fid, info) in self.breakpoints.clone() {
            self.enable_function_breakpoints_for(fid, &info);
        }
    }

    /// Replaces source breakpoints for a file id.
    pub fn set_source_breakpoints(
        &mut self,
        fid: FileId,
        breakpoints: impl IntoIterator<Item = SourceBreakpoint>,
    ) {
        let breakpoints = breakpoints.into_iter().collect::<Vec<_>>();

        if breakpoints.is_empty() {
            self.source_breakpoints.remove(&fid);
        } else {
            self.source_breakpoints.insert(fid, breakpoints);
        }

        self.enabled_source_breakpoints
            .retain(|(enabled_fid, _, _)| *enabled_fid != fid);
    }

    /// Replaces source breakpoints for a source and resolves them if metadata is available.
    pub fn set_source_breakpoints_for(
        &mut self,
        source: &Source,
        breakpoints: impl IntoIterator<Item = SourceBreakpoint>,
    ) -> Vec<SourceBreakpointResolution> {
        let fid = source.id();
        self.set_source_breakpoints(fid, breakpoints);

        let Some(info) = self.breakpoints.get(&fid).cloned() else {
            return self.pending_source_breakpoints(fid);
        };

        self.enable_source_breakpoints_for(source, &info)
    }

    fn enable_breakpoints_for(
        &mut self,
        source: &Source,
        info: &BreakpointInfo,
    ) -> Vec<SourceBreakpointResolution> {
        let fid = source.id();
        self.enable_function_breakpoints_for(fid, info);
        self.enable_source_breakpoints_for(source, info)
    }

    fn enable_function_breakpoints_for(&mut self, fid: FileId, info: &BreakpointInfo) {
        for (id, item) in info.meta.iter().enumerate() {
            if !matches!(item.kind, BreakpointKind::Function) {
                continue;
            }

            if item
                .function_name
                .as_ref()
                .is_some_and(|name| self.function_breakpoints.contains(name))
            {
                self.enabled_function_breakpoints
                    .insert((fid, id, BreakpointKind::Function));
            }
        }
    }

    fn enable_source_breakpoints_for(
        &mut self,
        source: &Source,
        info: &BreakpointInfo,
    ) -> Vec<SourceBreakpointResolution> {
        let fid = source.id();
        self.enabled_source_breakpoints
            .retain(|(enabled_fid, _, _)| *enabled_fid != fid);

        let Some(breakpoints) = self.source_breakpoints.get(&fid).cloned() else {
            return vec![];
        };

        let candidates = SourceBreakpointCandidate::collect(source, info);

        breakpoints
            .into_iter()
            .map(|requested| {
                let resolved =
                    best_source_breakpoint_candidate(&requested, &candidates).map(|candidate| {
                        self.enabled_source_breakpoints
                            .insert((fid, candidate.id, candidate.kind));

                        ResolvedSourceBreakpoint {
                            id: candidate.id,
                            kind: candidate.kind,
                            line: candidate.line,
                            column: candidate.column,
                        }
                    });

                SourceBreakpointResolution {
                    requested,
                    resolved,
                    pending: false,
                }
            })
            .collect()
    }

    fn pending_source_breakpoints(&self, fid: FileId) -> Vec<SourceBreakpointResolution> {
        self.source_breakpoints
            .get(&fid)
            .into_iter()
            .flatten()
            .cloned()
            .map(|requested| SourceBreakpointResolution {
                requested,
                resolved: None,
                pending: true,
            })
            .collect()
    }

    fn breakpoint_enabled(&self, bp: &(FileId, usize, BreakpointKind)) -> bool {
        self.enabled_function_breakpoints.contains(bp)
            || self.enabled_source_breakpoints.contains(bp)
    }
}

#[derive(Debug, Clone, Copy)]
struct SourceBreakpointCandidate {
    id: usize,
    kind: BreakpointKind,
    line: u32,
    column: u32,
}

impl SourceBreakpointCandidate {
    fn collect(source: &Source, info: &BreakpointInfo) -> Vec<Self> {
        info.meta
            .iter()
            .enumerate()
            .filter_map(|(id, item)| {
                if !is_source_breakpoint_target(item.kind) {
                    return None;
                }

                let range = source_range(source, item.origin_span)?;
                let position = to_lsp_position(range.start, PositionEncoding::Utf16, source);

                Some(Self {
                    id,
                    kind: item.kind,
                    line: position.line,
                    column: position.character,
                })
            })
            .collect()
    }
}

fn best_source_breakpoint_candidate(
    breakpoint: &SourceBreakpoint,
    candidates: &[SourceBreakpointCandidate],
) -> Option<SourceBreakpointCandidate> {
    candidates
        .iter()
        .min_by_key(|candidate| {
            let line_bucket = if matches!(candidate.kind, BreakpointKind::BlockEnd) {
                3
            } else if candidate.line == breakpoint.line {
                0
            } else if candidate.line > breakpoint.line {
                1
            } else {
                2
            };
            let column = breakpoint.column.unwrap_or(0);

            (
                line_bucket,
                candidate.line.abs_diff(breakpoint.line),
                source_breakpoint_kind_priority(candidate.kind),
                candidate.column.abs_diff(column),
                candidate.id,
            )
        })
        .copied()
}

fn is_source_breakpoint_target(kind: BreakpointKind) -> bool {
    matches!(
        kind,
        BreakpointKind::Function
            | BreakpointKind::BlockStart
            | BreakpointKind::ShowStart
            | BreakpointKind::BlockEnd
    )
}

fn source_breakpoint_kind_priority(kind: BreakpointKind) -> u8 {
    match kind {
        BreakpointKind::Function => 0,
        BreakpointKind::BlockStart => 1,
        BreakpointKind::ShowStart => 2,
        BreakpointKind::BlockEnd => 3,
        _ => 4,
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

    if session.is_some() && lock.is_some() {
        return false;
    }

    let _ = std::mem::replace(&mut *lock, session);
    true
}

/// Updates function breakpoints for the active debug session, if any.
pub fn set_debug_function_breakpoints(names: impl IntoIterator<Item = String>) -> bool {
    let mut session = DEBUG_SESSION.write();
    let Some(session) = session.as_mut() else {
        return false;
    };

    session.set_function_breakpoints(names);
    true
}

/// Updates source breakpoints for an active debug session, if any.
pub fn set_debug_source_breakpoints(
    source: Source,
    breakpoints: impl IntoIterator<Item = SourceBreakpoint>,
) -> Option<Vec<SourceBreakpointResolution>> {
    let mut session = DEBUG_SESSION.write();
    let session = session.as_mut()?;

    Some(session.set_source_breakpoints_for(&source, breakpoints))
}

/// Software breakpoints
fn check_soft_breakpoint(span: Span, id: usize, kind: BreakpointKind) -> Option<bool> {
    let fid = span.id()?;

    let session = DEBUG_SESSION.read();
    let session = session.as_ref()?;

    let bp_feature = (fid, id, kind);
    Some(session.breakpoint_enabled(&bp_feature))
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
        if !session.breakpoint_enabled(&bp_feature) {
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
