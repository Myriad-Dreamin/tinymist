use clap::Parser;
use criterion::Criterion;
use tinymist_world::CompileOnceArgs;
use typst::{
    engine::{Engine, Route, Sink, Traced},
    foundations::{Context, Value},
    introspection::Introspector,
    World,
};
use typst_syntax::{FileId, VirtualPath};

fn fibonacci(n: u64) -> u64 {
    match n {
        0 => 1,
        1 => 1,
        n => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

fn criterion_benchmark(c: &mut Criterion, world: &mut tinymist_world::LspWorld) {
    use comemo::Track;

    let id = FileId::new(None, VirtualPath::new("target/test-bench.typ"));

    let source = world.source(id).unwrap();
    let route = Route::default();
    let traced = Traced::default();
    let mut sink = Sink::default();
    let introspector = Introspector::default();

    let module = typst::eval::eval(
        ((world) as &dyn World).track(),
        traced.track(),
        sink.track_mut(),
        route.track(),
        &source,
    );

    let engine = &mut Engine {
        world: ((world) as &dyn World).track(),
        introspector: introspector.track(),
        traced: traced.track(),
        sink: sink.track_mut(),
        route,
    };

    let module = module.unwrap();

    let fib_func = module.scope().get("fib-bench").unwrap();
    let fib_func = match fib_func {
        Value::Func(f) => f,
        _ => panic!("Expected function"),
    };

    let mut call_typst_func_trampoline = || {
        let context = Context::default();
        let values = Vec::<Value>::default();
        let _result = fib_func.call(engine, context.track(), values).unwrap();
    };

    c.bench_function("fib typst", |b| {
        b.iter(|| {
            comemo::evict(0);
            call_typst_func_trampoline();
        })
    });
    c.bench_function("fib rust", |b| {
        b.iter(|| fibonacci(std::hint::black_box(20)))
    });
}

/// Common arguments of compile, watch, and query.
#[derive(Debug, Clone, Parser, Default)]
pub struct CompileArgs {
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Path to output file
    #[clap(value_name = "OUTPUT")]
    pub output: Option<String>,
}

pub fn run() -> anyhow::Result<()> {
    let crityp_path = std::env::current_dir().unwrap().join("target/crityp");

    // Parse command line arguments
    let mut args = CompileArgs::parse();

    args.compile.input = Some("target/test-bench.typ".to_string());

    let universe = args.compile.resolve()?;
    let mut world = universe.snapshot();

    let mut crit = criterion::Criterion::default().output_directory(&crityp_path);

    criterion_benchmark(&mut crit, &mut world);

    crit.final_summary();

    Ok(())
}
