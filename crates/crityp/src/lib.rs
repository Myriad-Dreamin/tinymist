use anyhow::Context as ContextTrait;
use comemo::Track;
use criterion::Criterion;
use ecow::{eco_format, EcoString};
use tinymist_world::reflexo_typst::path::unix_slash;
use tinymist_world::LspWorld;
use typst::engine::{Engine, Route, Sink, Traced};
use typst::foundations::{Context, Func, Value};
use typst::introspection::Introspector;
use typst::World;

pub fn bench(c: &mut Criterion, world: &mut LspWorld) -> anyhow::Result<()> {
    let main_source = world.source(world.main())?;
    let main_path = unix_slash(world.main().vpath().as_rooted_path());

    let route = Route::default();
    let mut sink = Sink::default();
    let traced = Traced::default();
    let introspector = Introspector::default();

    let module = typst::eval::eval(
        ((world) as &dyn World).track(),
        traced.track(),
        sink.track_mut(),
        route.track(),
        &main_source,
    );
    let module = module
        .map_err(|e| anyhow::anyhow!("{e:?}"))
        .context("evaluation error")?;

    let mut goals: Vec<(EcoString, &Func)> = vec![];
    for (name, value, _) in module.scope().iter() {
        if !name.starts_with("bench") {
            continue;
        }

        if let Value::Func(func) = value {
            goals.push((eco_format!("{main_path}@{name}"), func));
        }
    }

    for (name, func) in goals {
        let route = Route::default();
        let mut sink = Sink::default();
        let engine = &mut Engine {
            world: ((world) as &dyn World).track(),
            introspector: introspector.track(),
            traced: traced.track(),
            sink: sink.track_mut(),
            route,
        };

        let mut call_once = move || {
            let context = Context::default();
            let values = Vec::<Value>::default();
            func.call(engine, context.track(), values)
        };

        if let Err(err) = call_once() {
            eprintln!("call error in {name}: {err:?}");
            continue;
        }

        c.bench_function(&name, move |b| {
            b.iter(|| {
                comemo::evict(0);
                let _result = call_once();
            })
        });
    }

    Ok(())
}
