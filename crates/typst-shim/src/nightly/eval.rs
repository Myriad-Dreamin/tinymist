//! Typst Evaluation

use comemo::Track;
use typst::engine::{Route, Sink, Traced};
use typst::foundations::Module;
use typst::World;

/// Evaluates a source file and return the resulting module.
pub fn eval_compat(
    world: &dyn World,
    source: &typst::syntax::Source,
) -> typst::diag::SourceResult<Module> {
    let route = Route::default();
    let traced = Traced::default();
    let mut sink = Sink::default();

    typst_eval::eval(
        &typst::ROUTINES,
        world.track(),
        traced.track(),
        sink.track_mut(),
        route.track(),
        source,
    )
}
