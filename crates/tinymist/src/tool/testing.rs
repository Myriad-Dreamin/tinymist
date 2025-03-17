//! Testing utilities

use std::{path::Path, sync::Arc};

use comemo::Track;
use parking_lot::Mutex;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use reflexo_typst::{vfs::FileId, Bytes, LazyHash, SourceWorld, TypstDocument, TypstHtmlDocument};
use tinymist_project::world::{system::print_diagnostics, DiagnosticFormat};
use tinymist_query::{
    analysis::Analysis,
    syntax::{find_source_by_expr, node_ancestors},
    testing::{TestCaseKind, TestSuites},
};
use tinymist_std::{bail, error::prelude::*, typst::TypstPagedDocument};
use typst::{
    diag::{FileResult, SourceDiagnostic, SourceResult, Warned},
    ecow::EcoVec,
    engine::{Engine, Route, Sink, Traced},
    foundations::{Context, Datetime, Label, Value},
    introspection::Introspector,
    syntax::{ast, LinkedNode, Source, Span},
    text::{Font, FontBook},
    utils::PicoStr,
    Library, World,
};

use crate::project::*;

/// Runs coverage test on a document
pub fn coverage_main(args: CompileOnceArgs) -> Result<()> {
    // Prepares for the compilation
    let universe = args.resolve()?;
    let world = universe.snapshot();

    let result = Ok(()).and_then(|_| -> Result<()> {
        let res = tinymist_debug::collect_coverage::<TypstPagedDocument, _>(&world)?;
        let cov_path = Path::new("target/coverage.json");
        let res = serde_json::to_string(&res.to_json(&world)).context("coverage")?;

        std::fs::create_dir_all(cov_path.parent().context("parent")?).context("create coverage")?;
        std::fs::write(cov_path, res).context("write coverage")?;

        Ok(())
    });

    print_diag_or_error(&world, result)
}

/// Runs tests on a document
pub fn test_main(args: CompileOnceArgs) -> Result<()> {
    // Prepares for the compilation
    let universe = args.resolve()?;
    let world = universe.snapshot();

    let result = Ok(()).and_then(|_| -> Result<()> {
        let analysis = Analysis::default();

        let mut ctx = analysis.snapshot(world.clone());
        let doc = typst::compile::<TypstPagedDocument>(&ctx.world).output?;

        let suites =
            tinymist_query::testing::test_suites(&mut ctx, &TypstDocument::from(Arc::new(doc)))
                .context("failed to find suites")?;
        eprintln!(
            "Found {} tests and {} examples",
            suites.tests.len(),
            suites.examples.len()
        );

        let (cov, result) = tinymist_debug::with_cov(&world, |world| {
            let suites = suites.recheck(world);
            print_diag_or_error(world, TestRunner::new(&world, &suites).run())
        });
        let cov = cov?;
        let cov_path = Path::new("target/coverage.json");
        let res = serde_json::to_string(&cov.to_json(&world)).context("coverage")?;

        std::fs::create_dir_all(cov_path.parent().context("parent")?).context("create coverage")?;
        std::fs::write(cov_path, res).context("write coverage")?;

        result?;
        eprintln!("All test cases passed...");
        eprintln!("Written coverage to {}...", cov_path.display());
        Ok(())
    });

    print_diag_or_error(&world, result)
}

struct TestRunner<'a> {
    world: &'a dyn World,
    suites: &'a TestSuites,
    diagnostics: Mutex<Vec<EcoVec<SourceDiagnostic>>>,
}

impl<'a> TestRunner<'a> {
    fn new(world: &'a dyn World, suites: &'a TestSuites) -> Self {
        Self {
            world,
            suites,
            diagnostics: Mutex::new(Vec::new()),
        }
    }

    fn collect_diag<T>(&self, result: Warned<SourceResult<T>>) -> Option<T> {
        if !result.warnings.is_empty() {
            self.diagnostics.lock().push(result.warnings);
        }

        match result.output {
            Ok(v) => Some(v),
            Err(e) => {
                self.diagnostics.lock().push(e);
                None
            }
        }
    }

    fn run(self) -> Result<()> {
        rayon::in_place_scope(|s| {
            s.spawn(|_| {
                self.suites.tests.par_iter().for_each(|test| {
                    let name = &test.name;
                    eprintln!("Running test: {name}");
                    let world = with_main(self.world, test.location);
                    let introspector = Introspector::default();
                    let traced = Traced::default();
                    let route = Route::default();
                    let mut sink = Sink::default();
                    let engine = &mut Engine {
                        routines: &typst::ROUTINES,
                        world: ((&world) as &dyn World).track(),
                        introspector: introspector.track(),
                        traced: traced.track(),
                        sink: sink.track_mut(),
                        route,
                    };

                    let func = &test.function;

                    // Runs the benchmark once.
                    let mut call_once = move || {
                        let context = Context::default();
                        let values = Vec::<Value>::default();
                        func.call(engine, context.track(), values)
                    };

                    // Executes the function
                    match test.kind {
                        TestCaseKind::Test | TestCaseKind::Bench => {
                            if let Err(err) = call_once() {
                                eprintln!("call error in {name}: {err:?}");
                            } else {
                                eprintln!("Passed {name}");
                            }
                        }
                        TestCaseKind::Panic => match call_once() {
                            Ok(..) => {
                                eprintln!("{name} exited normally, expected panic");
                            }
                            Err(err) => {
                                let has_panic = err.iter().any(|p| p.message.contains("panic"));

                                if !has_panic {
                                    eprintln!("{name} exited with error, expected panic");
                                    self.diagnostics.lock().push(err);
                                } else {
                                    eprintln!("Passed {name}");
                                }
                            }
                        },
                        TestCaseKind::Example => {
                            let example =
                                get_example_file(&world, name, test.location, func.span());
                            match example {
                                Err(err) => {
                                    eprintln!("cannot find example file in {name}: {err:?}");
                                    return;
                                }
                                Ok(example) => self.run_example(&example),
                            };
                        }
                    }
                    comemo::evict(30);
                });
                self.suites.examples.par_iter().for_each(|test| {
                    self.run_example(test);
                    comemo::evict(30);
                });
            });
        });

        {
            let diagnostics = self.diagnostics.into_inner();
            if !diagnostics.is_empty() {
                let diags = diagnostics
                    .into_iter()
                    .flat_map(|e| e.into_iter())
                    .collect::<EcoVec<_>>();
                Err(diags)?
            }
        }
        Ok(())
    }

    fn run_example(&self, test: &Source) {
        let example = test.id().vpath().as_rooted_path().display();
        eprintln!("Running example: {example}",);
        let world = with_main(self.world, test.id());
        let mut has_err = false;
        let doc = self.collect_diag(typst::compile::<TypstPagedDocument>(&world));
        has_err |= doc.is_none();
        if self.can_html(doc) {
            let doc = self.collect_diag(typst::compile::<TypstHtmlDocument>(&world));
            has_err |= doc.is_none();
        }

        if has_err {
            return;
        }
        eprintln!("Passed {example}");
    }

    fn can_html(&self, doc: Option<TypstPagedDocument>) -> bool {
        let Some(doc) = doc else {
            return false;
        };

        let label = Label::new(PicoStr::intern("test-html-example"));
        // todo: error multiple times
        doc.introspector.query_label(label).is_ok()
    }
}

fn with_main(world: &dyn World, id: FileId) -> WorldWithMain<'_> {
    WorldWithMain { world, main: id }
}

struct WorldWithMain<'a> {
    world: &'a dyn World,
    main: FileId,
}

impl typst::World for WorldWithMain<'_> {
    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.world.source(id)
    }

    fn library(&self) -> &LazyHash<Library> {
        self.world.library()
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.world.book()
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.world.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.world.font(index)
    }

    fn today(&self, offset: Option<i64>) -> Option<Datetime> {
        self.world.today(offset)
    }
}

fn get_example_file(world: &dyn World, name: &str, id: FileId, span: Span) -> Result<Source> {
    let source = world.source(id).context_ut("cannot find file")?;
    let node = LinkedNode::new(source.root());
    let leaf = node.find(span).context("cannot find example function")?;
    let function = node_ancestors(&leaf)
        .find(|n| n.is::<ast::Closure>())
        .context("cannot find example function")?;
    let closure = function.cast::<ast::Closure>().unwrap();
    if closure.params().children().count() != 0 {
        bail!("example function must not have parameters");
    }
    let included =
        find_include_expr(name, closure.body()).context("cannot find example function")?;
    find_source_by_expr(world, id, included).context("cannot find example file")
}

fn find_include_expr<'a>(name: &str, node: ast::Expr<'a>) -> Option<ast::Expr<'a>> {
    match node {
        ast::Expr::Include(inc) => Some(inc.source()),
        ast::Expr::Code(code) => {
            let exprs = code.body();
            if exprs.exprs().count() != 1 {
                eprintln!("example function must have a single inclusion: {name}");
                return None;
            }
            find_include_expr(name, exprs.exprs().next().unwrap())
        }
        _ => {
            eprintln!("example function must have a single inclusion: {name}");
            None
        }
    }
}

fn print_diag_or_error(world: &impl SourceWorld, result: Result<()>) -> Result<()> {
    if let Err(e) = result {
        if let Some(diagnostics) = e.diagnostics() {
            print_diagnostics(world, diagnostics.iter(), DiagnosticFormat::Human)
                .context_ut("print diagnostics")?;
            bail!("");
        }

        return Err(e);
    }

    Ok(())
}
