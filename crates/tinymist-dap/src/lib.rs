//! Fast debugger implementation for typst.

//       this._runtime = new MockRuntime(fileAccessor);

//       // setup event handlers
//       this._runtime.on("stopOnEntry", () => {
//         this.sendEvent(new StoppedEvent("entry",
// TypstDebugSession.threadID));       });
//       this._runtime.on("stopOnStep", () => {
//         this.sendEvent(new StoppedEvent("step", TypstDebugSession.threadID));
//       });
//       this._runtime.on("stopOnBreakpoint", () => {
//         this.sendEvent(new StoppedEvent("breakpoint",
// TypstDebugSession.threadID));       });
//       this._runtime.on("stopOnException", (exception) => {
//         if (exception) {
//           this.sendEvent(new StoppedEvent(`exception(${exception})`,
// TypstDebugSession.threadID));         } else {
//           this.sendEvent(new StoppedEvent("exception",
// TypstDebugSession.threadID));         }
//       });
//       this._runtime.on("breakpointValidated", (bp: IRuntimeBreakpoint) => {
//         this.sendEvent(
//           new BreakpointEvent("changed", {
//             verified: bp.verified,
//             id: bp.id,
//           } as DebugProtocol.Breakpoint),
//         );
//       });
//       this._runtime.on("output", (type, text, filePath, line, column) => {
//         let category: string;
//         switch (type) {
//           case "prio":
//             category = "important";
//             break;
//           case "out":
//             category = "stdout";
//             break;
//           case "err":
//             category = "stderr";
//             break;
//           default:
//             category = "console";
//             break;
//         }
//         const e: DebugProtocol.OutputEvent = new OutputEvent(`${text}\n`,
// category);

//         if (text === "start" || text === "startCollapsed" || text === "end")
// {           e.body.group = text;
//           e.body.output = `group-${text}\n`;
//         }

//         e.body.source = this.createSource(filePath);
//         e.body.line = this.convertDebuggerLineToClient(line);
//         e.body.column = this.convertDebuggerColumnToClient(column);
//         this.sendEvent(e);
//       });
//       this._runtime.on("end", () => {
//         this.sendEvent(new TerminatedEvent());
//       });

pub use tinymist_debug::BreakpointKind;

use std::sync::{mpsc, Arc};

use comemo::Track;
use comemo::Tracked;
use parking_lot::Mutex;
use tinymist_debug::{set_debug_session, DebugSession, DebugSessionHandler};
use tinymist_std::typst_shim::eval::{Eval, Vm};
use tinymist_world::{CompilerFeat, CompilerWorld};
use typst::{
    diag::{SourceResult, Warned},
    engine::{Engine, Route, Sink, Traced},
    foundations::{Context, Scopes, Value},
    introspection::Introspector,
    layout::PagedDocument,
    syntax::{ast, parse_code, Span},
    World, __bail as bail,
};

type RequestId = i64;

/// A debug request.
pub enum DebugRequest {
    /// Evaluates an expression.
    Evaluate(RequestId, String),
    /// Continues the execution.
    Continue,
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
    /// Responds to a debug request.
    fn respond(&self, id: RequestId, result: SourceResult<Value>);
}

/// Starts a debug session.
pub fn start_session<F: CompilerFeat>(
    base: CompilerWorld<F>,
    adaptor: Arc<dyn DebugAdaptor>,
    rx: mpsc::Receiver<DebugRequest>,
) {
    let context = Arc::new(DebugContext {});

    std::thread::spawn(move || {
        let world = tinymist_debug::instr_breakpoints(&base);

        if !set_debug_session(Some(DebugSession::new(context))) {
            adaptor.terminate();
            return None;
        }

        let _lock = ResourceLock::new(adaptor.clone(), rx);

        adaptor.before_compile();
        step_global(BreakpointKind::BeforeCompile, &world);

        let result = typst::compile::<PagedDocument>(&world);

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

    let introspector = Introspector::default();
    let traced = Traced::default();
    let mut sink = Sink::default();
    let route = Route::default();

    let engine = Engine {
        routines: &typst::ROUTINES,
        world: world.track(),
        introspector: introspector.track(),
        traced: traced.track(),
        sink: sink.track_mut(),
        route,
    };

    let context = Context::default();

    let span = Span::detached();

    let context = BreakpointContext {
        engine: &engine,
        context: context.track(),
        scopes: Scopes::new(Some(world.library())),
        span,
        kind,
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
}

impl BreakpointContext<'_, '_, '_> {
    fn evaluate(&self, expr: &str) -> SourceResult<Value> {
        let mut root = parse_code(expr);
        root.synthesize(self.span);

        // Check for well-formedness.
        let errors = root.errors();
        if !errors.is_empty() {
            return Err(errors.into_iter().map(Into::into).collect());
        }

        // Prepare VM.
        let mut sink = Sink::new();
        let engine = Engine {
            world: self.engine.world,
            introspector: self.engine.introspector,
            traced: self.engine.traced,
            routines: self.engine.routines,
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

fn step(ctx: &BreakpointContext, resource: &mut Resource) {
    resource.adaptor.stopped(ctx);
    loop {
        match resource.rx.recv() {
            Ok(DebugRequest::Evaluate(id, expr)) => {
                let res = ctx.evaluate(&expr);
                eprintln!("evaluate: {expr} => {res:?}");
                resource.adaptor.respond(id, res);
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
    ) {
        let mut resource = RESOURCES.lock();
        let context = BreakpointContext {
            engine,
            context,
            scopes,
            span,
            kind,
        };
        step(&context, resource.as_mut().unwrap());
    }
}
