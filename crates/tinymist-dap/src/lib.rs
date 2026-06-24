//! Fast debugger implementation for typst.

//       this._runtime = new MockRuntime(fileAccessor);

//       this._runtime.on("breakpointValidated", (bp: IRuntimeBreakpoint) => {
//         this.sendEvent(
//           new BreakpointEvent("changed", {
//             verified: bp.verified,
//             id: bp.id,
//           } as DebugProtocol.Breakpoint),
//         );
//       });
//       this._runtime.on("end", () => {
//         this.sendEvent(new TerminatedEvent());
//       });

pub use tinymist_debug::BreakpointKind;

use std::{
    collections::HashMap,
    sync::{Arc, mpsc},
};

use comemo::Track;
use comemo::Tracked;
use parking_lot::Mutex;
use tinymist_debug::{DebugSession, DebugSessionHandler, SourceBreakpoint, set_debug_session};
use tinymist_std::typst_shim::eval::{Eval, Vm};
use tinymist_world::{CompilerFeat, CompilerWorld, vfs::FileId};
use typst::{
    __bail as bail, World,
    diag::{SourceResult, Warned},
    engine::{Engine, Route, Sink, Traced},
    foundations::{Binding, Context, Repr, Scope, Scopes, Value},
    introspection::EmptyIntrospector,
    syntax::{Span, ast, parse_code},
};
use typst_layout::PagedDocument;

type RequestId = i64;

/// A debug request.
pub enum DebugRequest {
    /// Evaluates an expression.
    Evaluate(RequestId, String),
    /// Retrieves stack frames.
    StackTrace(RequestId, DebugStackTraceArguments),
    /// Retrieves variable scopes for a stack frame.
    Scopes(RequestId, DebugScopesArguments),
    /// Retrieves variables for a variables reference.
    Variables(RequestId, DebugVariablesArguments),
    /// Continues the execution.
    Continue,
}

/// Arguments for a stack trace request.
#[derive(Debug, Clone, Default)]
pub struct DebugStackTraceArguments {
    /// The index of the first frame to return.
    pub start_frame: Option<u64>,
    /// The maximum number of frames to return. `None` or zero means all frames.
    pub levels: Option<u64>,
}

/// Arguments for a scopes request.
#[derive(Debug, Clone)]
pub struct DebugScopesArguments {
    /// The requested frame id.
    pub frame_id: u64,
}

/// Arguments for a variables request.
#[derive(Debug, Clone)]
pub struct DebugVariablesArguments {
    /// The requested variables reference.
    pub variables_reference: u32,
    /// The index of the first variable to return.
    pub start: Option<u64>,
    /// The maximum number of variables to return. `None` or zero means all variables.
    pub count: Option<u64>,
    /// Optional child filter.
    pub filter: Option<DebugVariablesFilter>,
}

/// Variable child filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugVariablesFilter {
    /// Return indexed children.
    Indexed,
    /// Return named children.
    Named,
}

/// Result type for debugger data requests.
pub type DebugResult<T> = Result<T, String>;

/// A stack trace response.
#[derive(Debug, Clone, Default)]
pub struct DebugStackTraceResponse {
    /// Stack frames available for the current pause.
    pub stack_frames: Vec<DebugStackFrame>,
    /// The total number of frames available before paging.
    pub total_frames: u64,
}

/// A stack frame.
#[derive(Debug, Clone)]
pub struct DebugStackFrame {
    /// Stable frame id for the current pause.
    pub id: u64,
    /// Frame display name.
    pub name: String,
    /// Source span for the frame.
    pub span: Span,
    /// Breakpoint kind that produced this frame.
    pub kind: BreakpointKind,
}

/// A scopes response.
#[derive(Debug, Clone, Default)]
pub struct DebugScopesResponse {
    /// Scopes available for the requested frame.
    pub scopes: Vec<DebugScope>,
}

/// A variable scope.
#[derive(Debug, Clone)]
pub struct DebugScope {
    /// Scope display name.
    pub name: String,
    /// Variables reference for the scope.
    pub variables_reference: u32,
    /// Number of named variables.
    pub named_variables: Option<u32>,
    /// Number of indexed variables.
    pub indexed_variables: Option<u32>,
    /// Whether variables in this scope are expensive to fetch.
    pub expensive: bool,
    /// Optional scope presentation hint.
    pub presentation_hint: Option<DebugScopePresentationHint>,
    /// Source span covered by this scope, if known.
    pub span: Span,
}

/// Scope presentation hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugScopePresentationHint {
    /// Scope contains function arguments.
    Arguments,
    /// Scope contains local variables.
    Locals,
}

/// A variables response.
#[derive(Debug, Clone, Default)]
pub struct DebugVariablesResponse {
    /// Variables returned for the requested reference.
    pub variables: Vec<DebugVariable>,
}

/// A variable.
#[derive(Debug, Clone)]
pub struct DebugVariable {
    /// Variable display name.
    pub name: String,
    /// Variable value representation.
    pub value: String,
    /// Variable type representation.
    pub ty: Option<String>,
    /// Child variables reference, or zero if this value has no children.
    pub variables_reference: u32,
    /// Number of named child variables.
    pub named_variables: Option<u32>,
    /// Number of indexed child variables.
    pub indexed_variables: Option<u32>,
    /// Evaluatable expression for this variable.
    pub evaluate_name: Option<String>,
}

/// A handler for debug events.
pub trait DebugAdaptor: Send + Sync {
    /// Called before the compilation.
    fn before_compile(&self);
    /// Called after the compilation.
    fn after_compile(&self, result: Warned<SourceResult<PagedDocument>>);
    /// Terminates the debug session.
    fn terminate(&self);
    /// Responds to a debug request.
    fn stopped(&self, ctx: &BreakpointContext);
    /// Responds to an evaluate request.
    fn respond_evaluate(&self, id: RequestId, result: SourceResult<Value>);
    /// Responds to a stack trace request.
    fn respond_stack_trace(&self, id: RequestId, result: DebugResult<DebugStackTraceResponse>);
    /// Responds to a scopes request.
    fn respond_scopes(&self, id: RequestId, result: DebugResult<DebugScopesResponse>);
    /// Responds to a variables request.
    fn respond_variables(&self, id: RequestId, result: DebugResult<DebugVariablesResponse>);
}

/// Starts a debug session.
pub fn start_session<F: CompilerFeat>(
    base: CompilerWorld<F>,
    adaptor: Arc<dyn DebugAdaptor>,
    rx: mpsc::Receiver<DebugRequest>,
    function_breakpoints: Vec<String>,
    source_breakpoints: Vec<(FileId, Vec<SourceBreakpoint>)>,
) {
    let context = Arc::new(DebugContext {});

    std::thread::spawn(move || {
        let world = tinymist_debug::instr_breakpoints(&base);

        let mut session = DebugSession::new(context);
        session.set_function_breakpoints(function_breakpoints);
        for (fid, breakpoints) in source_breakpoints {
            session.set_source_breakpoints(fid, breakpoints);
        }

        if !set_debug_session(Some(session)) {
            adaptor.terminate();
            return None;
        }

        let _lock = ResourceLock::new(adaptor.clone(), rx);

        adaptor.before_compile();
        step_global(BreakpointKind::BeforeCompile, &world);

        let result = typst_shim::compile_opt::<PagedDocument>(&world);

        adaptor.after_compile(result);
        step_global(BreakpointKind::AfterCompile, &world);

        *RESOURCES.lock() = None;
        set_debug_session(None);

        adaptor.terminate();
        Some(())
    });
}

static RESOURCES: Mutex<Option<Resource>> = Mutex::new(None);

struct Resource {
    adaptor: Arc<dyn DebugAdaptor>,
    rx: mpsc::Receiver<DebugRequest>,
}

struct ResourceLock;

impl ResourceLock {
    fn new(adaptor: Arc<dyn DebugAdaptor>, rx: mpsc::Receiver<DebugRequest>) -> Self {
        RESOURCES.lock().replace(Resource { adaptor, rx });

        Self
    }
}

impl Drop for ResourceLock {
    fn drop(&mut self) {
        *RESOURCES.lock() = None;
    }
}

fn step_global(kind: BreakpointKind, world: &dyn World) {
    let mut resource = RESOURCES.lock();

    let library = world.library();
    let introspector = EmptyIntrospector;
    let traced = Traced::default();
    let mut sink = Sink::default();
    let route = Route::default();

    let engine = Engine {
        library,
        world: world.track(),
        introspector: typst::utils::Protected::new(introspector.track()),
        traced: traced.track(),
        sink: sink.track_mut(),
        route,
    };

    let context = Context::default();

    let span = Span::detached();

    let mut scopes = Scopes::new(Some(world.library()));
    if matches!(kind, BreakpointKind::AfterCompile) {
        let m = world
            .source(world.main())
            .ok()
            .and_then(|s| typst_shim::eval::eval_compat(world, &s).ok());

        if let Some(m) = m {
            scopes.top = m.scope().clone();
        }
    }

    let context = BreakpointContext {
        engine: &engine,
        context: context.track(),
        scopes,
        span,
        kind,
        function_name: None,
    };

    step(&context, resource.as_mut().unwrap());
}

/// A breakpoint context.
pub struct BreakpointContext<'a, 'b, 'c> {
    /// The breakpoint kind.
    pub kind: BreakpointKind,

    engine: &'a Engine<'c>,
    context: Tracked<'a, Context<'b>>,
    scopes: Scopes<'a>,
    span: Span,
    function_name: Option<String>,
}

impl BreakpointContext<'_, '_, '_> {
    /// The original source span that caused the breakpoint stop.
    pub fn source_span(&self) -> Span {
        self.span
    }

    /// The function name associated with this breakpoint, if known.
    pub fn function_name(&self) -> Option<&str> {
        self.function_name.as_deref()
    }

    fn evaluate(&self, expr: &str) -> SourceResult<Value> {
        let mut root = parse_code(expr);
        root.synthesize(self.span);

        // Check for well-formedness.
        let errors = root.errors_and_warnings().0;
        if !errors.is_empty() {
            return Err(errors.into_iter().map(Into::into).collect());
        }

        // Prepare VM.
        let mut sink = Sink::new();
        let engine = Engine {
            world: self.engine.world,
            library: self.engine.library,
            introspector: self.engine.introspector,
            traced: self.engine.traced,
            sink: sink.track_mut(),
            route: self.engine.route.clone(),
        };
        let mut vm = Vm::new(engine, self.context, self.scopes.clone(), root.span());

        // Evaluate the code.
        let output = root.cast::<ast::Code>().unwrap().eval(&mut vm)?;

        // Handle control flow.
        if let Some(flow) = vm.flow {
            bail!(flow.forbidden());
        }

        Ok(output)
    }
}

const CURRENT_FRAME_ID: u64 = 1;

#[derive(Clone)]
struct VariableEntry {
    name: String,
    value: Value,
    evaluate_name: Option<String>,
    indexed: bool,
}

#[derive(Clone)]
struct VariableHandle {
    variables: Vec<VariableEntry>,
}

struct ValueChildren {
    variables: Vec<VariableEntry>,
    named_variables: Option<u32>,
    indexed_variables: Option<u32>,
}

struct PausedState {
    frames: Vec<DebugStackFrame>,
    scopes: HashMap<u64, Vec<DebugScope>>,
    handles: Vec<VariableHandle>,
}

impl PausedState {
    fn new(ctx: &BreakpointContext) -> Self {
        let mut state = Self {
            frames: vec![DebugStackFrame {
                id: CURRENT_FRAME_ID,
                name: frame_name(ctx),
                span: ctx.span,
                kind: ctx.kind,
            }],
            scopes: HashMap::new(),
            handles: vec![],
        };

        let mut scopes = vec![];

        let top_variables = collect_scope_variables(&ctx.scopes.top);
        let (top_name, top_hint) = if matches!(ctx.kind, BreakpointKind::Function) {
            ("Arguments", DebugScopePresentationHint::Arguments)
        } else {
            ("Locals", DebugScopePresentationHint::Locals)
        };
        scopes.push(state.scope(top_name, top_variables, false, Some(top_hint), ctx.span));

        for (idx, scope) in ctx.scopes.scopes.iter().rev().enumerate() {
            let variables = collect_scope_variables(scope);
            if variables.is_empty() {
                continue;
            }

            scopes.push(state.scope(
                format!("Outer {}", idx + 1),
                variables,
                false,
                Some(DebugScopePresentationHint::Locals),
                ctx.span,
            ));
        }

        if let Some(base) = ctx.scopes.base {
            let mut variables = collect_scope_variables(base.global.scope());
            variables.push(binding_variable("std", &base.std));
            scopes.push(state.scope("Globals", variables, true, None, ctx.span));
        }

        state.scopes.insert(CURRENT_FRAME_ID, scopes);
        state
    }

    fn scope(
        &mut self,
        name: impl Into<String>,
        variables: Vec<VariableEntry>,
        expensive: bool,
        presentation_hint: Option<DebugScopePresentationHint>,
        span: Span,
    ) -> DebugScope {
        let named_variables = count_variables(variables.iter().filter(|it| !it.indexed).count());
        let indexed_variables = count_variables(variables.iter().filter(|it| it.indexed).count());
        let variables_reference = self.add_handle(variables);

        DebugScope {
            name: name.into(),
            variables_reference,
            named_variables,
            indexed_variables,
            expensive,
            presentation_hint,
            span,
        }
    }

    fn stack_trace(&self, args: DebugStackTraceArguments) -> DebugStackTraceResponse {
        let start = args.start_frame.unwrap_or_default().min(usize::MAX as u64) as usize;
        let count = args.levels.filter(|levels| *levels > 0);
        let frames = paged(self.frames.iter().cloned(), start, count).collect();

        DebugStackTraceResponse {
            stack_frames: frames,
            total_frames: self.frames.len().min(u64::MAX as usize) as u64,
        }
    }

    fn scopes(&self, args: DebugScopesArguments) -> DebugResult<DebugScopesResponse> {
        let Some(scopes) = self.scopes.get(&args.frame_id) else {
            return Err(format!("unknown stack frame: {}", args.frame_id));
        };

        Ok(DebugScopesResponse {
            scopes: scopes.clone(),
        })
    }

    fn variables(&mut self, args: DebugVariablesArguments) -> DebugResult<DebugVariablesResponse> {
        let Some(handle) = self.handle(args.variables_reference).cloned() else {
            return Err(format!(
                "unknown variables reference: {}",
                args.variables_reference
            ));
        };

        let start = args.start.unwrap_or_default().min(usize::MAX as u64) as usize;
        let count = args.count.filter(|count| *count > 0);
        let variables = paged(
            handle
                .variables
                .into_iter()
                .filter(|entry| match args.filter {
                    Some(DebugVariablesFilter::Indexed) => entry.indexed,
                    Some(DebugVariablesFilter::Named) => !entry.indexed,
                    None => true,
                }),
            start,
            count,
        )
        .map(|entry| self.variable(entry))
        .collect();

        Ok(DebugVariablesResponse { variables })
    }

    fn variable(&mut self, entry: VariableEntry) -> DebugVariable {
        let children = value_children(&entry.value);
        let (variables_reference, named_variables, indexed_variables) =
            if let Some(children) = children {
                (
                    self.add_handle(children.variables),
                    children.named_variables,
                    children.indexed_variables,
                )
            } else {
                (0, None, None)
            };

        DebugVariable {
            name: entry.name,
            value: entry.value.repr().to_string(),
            ty: Some(entry.value.ty().repr().to_string()),
            variables_reference,
            named_variables,
            indexed_variables,
            evaluate_name: entry.evaluate_name,
        }
    }

    fn add_handle(&mut self, variables: Vec<VariableEntry>) -> u32 {
        self.handles.push(VariableHandle { variables });
        self.handles.len().min(u32::MAX as usize) as u32
    }

    fn handle(&self, reference: u32) -> Option<&VariableHandle> {
        let index = reference.checked_sub(1)? as usize;
        self.handles.get(index)
    }
}

fn frame_name(ctx: &BreakpointContext) -> String {
    if let Some(name) = ctx.function_name() {
        return name.to_owned();
    }

    match ctx.kind {
        BreakpointKind::BeforeCompile => "document start".into(),
        BreakpointKind::AfterCompile => "document end".into(),
        BreakpointKind::Function => "function".into(),
        BreakpointKind::BlockStart | BreakpointKind::BlockEnd => "block".into(),
        BreakpointKind::ShowStart | BreakpointKind::ShowEnd => "show rule".into(),
        kind => kind.to_str().replace('_', " "),
    }
}

fn collect_scope_variables(scope: &Scope) -> Vec<VariableEntry> {
    scope
        .iter()
        .map(|(name, binding)| binding_variable(name.as_str(), binding))
        .collect()
}

fn binding_variable(name: &str, binding: &Binding) -> VariableEntry {
    VariableEntry {
        name: name.to_owned(),
        value: binding.read().clone(),
        evaluate_name: Some(name.to_owned()),
        indexed: false,
    }
}

fn value_children(value: &Value) -> Option<ValueChildren> {
    match value {
        Value::Array(array) => {
            let variables = array
                .iter()
                .enumerate()
                .map(|(idx, value)| VariableEntry {
                    name: idx.to_string(),
                    value: value.clone(),
                    evaluate_name: None,
                    indexed: true,
                })
                .collect::<Vec<_>>();
            if variables.is_empty() {
                return None;
            }

            Some(ValueChildren {
                indexed_variables: count_variables(variables.len()),
                named_variables: None,
                variables,
            })
        }
        Value::Dict(dict) => {
            let variables = dict
                .iter()
                .map(|(name, value)| VariableEntry {
                    name: name.as_str().to_owned(),
                    value: value.clone(),
                    evaluate_name: None,
                    indexed: false,
                })
                .collect::<Vec<_>>();
            if variables.is_empty() {
                return None;
            }

            Some(ValueChildren {
                named_variables: count_variables(variables.len()),
                indexed_variables: None,
                variables,
            })
        }
        value => {
            let scope = value.scope()?;
            let variables = collect_scope_variables(scope);
            if variables.is_empty() {
                return None;
            }

            Some(ValueChildren {
                named_variables: count_variables(variables.len()),
                indexed_variables: None,
                variables,
            })
        }
    }
}

fn count_variables(count: usize) -> Option<u32> {
    if count == 0 {
        None
    } else {
        Some(count.min(u32::MAX as usize) as u32)
    }
}

fn paged<T>(
    iter: impl Iterator<Item = T>,
    start: usize,
    count: Option<u64>,
) -> impl Iterator<Item = T> {
    iter.skip(start)
        .take(count.unwrap_or(u64::MAX).min(usize::MAX as u64) as usize)
}

fn step(ctx: &BreakpointContext, resource: &mut Resource) {
    let mut state = PausedState::new(ctx);

    resource.adaptor.stopped(ctx);
    loop {
        match resource.rx.recv() {
            Ok(DebugRequest::Evaluate(id, expr)) => {
                let res = ctx.evaluate(&expr);
                eprintln!("evaluate: {expr} => {res:?}");
                resource.adaptor.respond_evaluate(id, res);
            }
            Ok(DebugRequest::StackTrace(id, args)) => {
                resource
                    .adaptor
                    .respond_stack_trace(id, Ok(state.stack_trace(args)));
            }
            Ok(DebugRequest::Scopes(id, args)) => {
                resource.adaptor.respond_scopes(id, state.scopes(args));
            }
            Ok(DebugRequest::Variables(id, args)) => {
                resource
                    .adaptor
                    .respond_variables(id, state.variables(args));
            }
            Ok(DebugRequest::Continue) => {
                break;
            }
            Err(mpsc::RecvError) => {
                break;
            }
        }
    }
}

struct DebugContext {}

impl DebugSessionHandler for DebugContext {
    fn on_breakpoint(
        &self,
        engine: &Engine,
        context: Tracked<Context>,
        scopes: Scopes,
        span: Span,
        kind: BreakpointKind,
        function_name: Option<String>,
    ) {
        let mut resource = RESOURCES.lock();
        let context = BreakpointContext {
            engine,
            context,
            scopes,
            span,
            kind,
            function_name,
        };
        step(&context, resource.as_mut().unwrap());
    }
}
