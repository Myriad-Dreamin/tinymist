//! Testing utilities

use core::fmt;
use std::{
    collections::HashSet,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use comemo::Track;
use parking_lot::Mutex;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use reflexo::ImmutPath;
use reflexo_typst::{vfs::FileId, SourceWorld, TypstDocument, TypstHtmlDocument};
use tinymist_project::world::{system::print_diagnostics, DiagnosticFormat};
use tinymist_query::{
    analysis::Analysis,
    syntax::{find_source_by_expr, node_ancestors},
    testing::{TestCaseKind, TestSuites},
};
use tinymist_std::{bail, error::prelude::*, fs::paths::write_atomic, typst::TypstPagedDocument};
use typst::{
    diag::{SourceDiagnostic, SourceResult, Warned},
    ecow::EcoVec,
    engine::{Engine, Route, Sink, Traced},
    foundations::{Context, Label, Value},
    introspection::Introspector,
    syntax::{ast, LinkedNode, Source, Span},
    utils::PicoStr,
    World,
};

use super::project::{start_project, StartProjectResult};
use crate::world::with_main;
use crate::{project::*, utils::exit_on_ctrl_c};

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

/// Testing arguments
#[derive(Debug, Clone, clap::Parser)]
pub struct TestArgs {
    /// The argument to compile once.
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Configuration for testing
    #[clap(flatten)]
    pub config: TestConfigArgs,

    /// Whether to run in watch mode.
    #[clap(long)]
    pub watch: bool,

    /// Whether to log verbose information.
    #[clap(long)]
    pub verbose: bool,
}

/// Testing config arguments
#[derive(Debug, Clone, clap::Parser)]
pub struct TestConfigArgs {
    /// Whether to update the reference images.
    #[clap(long)]
    pub update: bool,

    /// The argument to export to PNG.
    #[clap(flatten)]
    pub png: PngExportArgs,
}

macro_rules! test_log {
    ($level:ident,$prefix:expr, $($arg:tt)*) => {
        msg(Level::$level, $prefix, format_args!($($arg)*))
    };
}

macro_rules! test_info {
    ($( $arg:tt )*) => {
        test_log!(Info, $($arg)*)
    };
}

macro_rules! test_error {
    ($( $arg:tt )*) => {
        test_log!(Error, $($arg)*)
    };
}

macro_rules! test_hint {
    ($( $arg:tt )*) => {
        test_log!(Hint, $($arg)*)
    };
}

/// Runs tests on a document
pub async fn test_main(args: TestArgs) -> Result<()> {
    exit_on_ctrl_c();

    // Prepares for the compilation
    let verse = args.compile.resolve()?;
    let analysis = Analysis::default();

    let root = verse.entry_state().root().map(Ok);
    let root = root
        .unwrap_or_else(|| std::env::current_dir().map(|p| p.into()))
        .context("cannot find root")?;

    let out_file = if args.watch {
        use std::io::Write;
        let mut out_file = std::fs::File::create("test-watch.typ").context("create log file")?;
        writeln!(out_file, "#import \"/target/test-template.typ\": *").context("write log")?;
        writeln!(out_file, "#show: main").context("write log")?;
        Some(Arc::new(Mutex::new(out_file)))
    } else {
        None
    };

    let config = TestConfig {
        root,
        args: args.config,
        out_file,
    };

    if !args.watch {
        let snap = verse.snapshot();
        return match test_once(&analysis, &snap, &config) {
            Ok(true) => Ok(()),
            Ok(false) | Err(..) => std::process::exit(1),
        };
    }

    let config = Arc::new(Mutex::new(config.clone()));
    let config_update = config.clone();

    let mut is_first = true;
    let StartProjectResult {
        service,
        mut editor_rx,
        intr_tx,
    } = start_project(verse, None, move |c, mut i, next| {
        if let Interrupt::Compiled(artifact) = &mut i {
            let mut config = config.lock();
            let instant = std::time::Instant::now();
            // todo: well term support
            // Clear the screen and then move the cursor to the top left corner.
            eprintln!("\x1B[2J\x1B[1;1H");

            if is_first {
                is_first = false;
            } else {
                test_info!("Info:", "Re-testing...");
            }
            // Sets is_compiling to track dependencies
            artifact.snap.world.set_is_compiling(true);
            let res = test_once(&analysis, &artifact.world, &config);
            artifact.snap.world.set_is_compiling(false);
            if let Err(err) = res {
                test_error!("Fatal:", "{err}");
            }
            test_info!("Info:", "Tests finished in {:?}", instant.elapsed());
            test_hint!("Hint:", "Press 'h' for help");
            config.args.update = false;
        }

        next(c, i)
    });

    let id = service.compiler.primary.id.clone();
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            std::io::stdin().read_line(&mut line).unwrap();
            match line.trim() {
                "h" => {
                    eprintln!("h/r/u/q: help/run/update/quit");
                }
                "r" => {
                    let _ = intr_tx.send(Interrupt::Compile(id.clone()));
                }
                "u" => {
                    let mut config = config_update.lock();
                    config.args.update = true;
                    let _ = intr_tx.send(Interrupt::Compile(id.clone()));
                }
                "q" => {
                    std::process::exit(0);
                }
                _ => {
                    println!("Unknown command");
                }
            }
        }
    });

    // Consume service and editor_rx
    tokio::spawn(async move { while editor_rx.recv().await.is_some() {} });

    service.run().await;

    Ok(())
}

fn test_once(analysis: &Analysis, world: &LspWorld, config: &TestConfig) -> Result<bool> {
    let mut ctx = analysis.snapshot(world.clone());
    let doc = typst::compile::<TypstPagedDocument>(&ctx.world).output?;

    let suites =
        tinymist_query::testing::test_suites(&mut ctx, &TypstDocument::from(Arc::new(doc)))
            .context("failed to find suites")?;
    test_info!(
        "Info:",
        "Found {} tests and {} examples",
        suites.tests.len(),
        suites.examples.len()
    );

    let (cov, result) = tinymist_debug::with_cov(world, |world| {
        let suites = suites.recheck(world);
        let runner = TestRunner::new(config.clone(), &world, &suites);
        print_diag_or_error(world, runner.run())
    });
    let cov = cov?;
    let cov_path = Path::new("target/coverage.json");
    let res = serde_json::to_string(&cov.to_json(world)).context("coverage")?;

    std::fs::create_dir_all(cov_path.parent().context("parent")?).context("create coverage")?;
    write_atomic(cov_path, res).context("write coverage")?;

    test_info!("Info:", "Written coverage to {} ...", cov_path.display());
    let passed = print_diag_or_error(world, result);

    if matches!(passed, Ok(true)) {
        test_info!("Info:", "All test cases passed...");
    } else {
        test_error!("Fatal:", "Some test cases failed...");
    }

    passed
}

#[derive(Debug, Clone)]
struct TestConfig {
    root: ImmutPath,
    args: TestConfigArgs,
    out_file: Option<Arc<Mutex<std::fs::File>>>,
}

struct TestRunner<'a> {
    config: TestConfig,
    world: &'a dyn World,
    suites: &'a TestSuites,
    diagnostics: Mutex<Vec<EcoVec<SourceDiagnostic>>>,
    examples: Mutex<HashSet<String>>,
    failed: AtomicBool,
}

impl<'a> TestRunner<'a> {
    fn new(config: TestConfig, world: &'a dyn World, suites: &'a TestSuites) -> Self {
        Self {
            config,
            world,
            suites,
            diagnostics: Mutex::new(Vec::new()),
            examples: Mutex::new(HashSet::new()),
            failed: AtomicBool::new(false),
        }
    }

    fn failed_example(&self, name: &str, args: impl fmt::Display) {
        self.mark_failed("example", name, args);
    }

    fn failed_test(&self, name: &str, args: impl fmt::Display) {
        self.mark_failed("test", name, args);
    }

    fn mark_failed(&self, kind: &str, name: &str, args: impl fmt::Display) {
        test_log!(Error, "Failed", "{kind}({name}): {args}");
        self.put_log(format_args!("#failed-{kind}({name:?})"));
        self.failed.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn passed_example(&self, name: &str) {
        self.mark_passed("example", name);
    }

    fn passed_test(&self, name: &str) {
        self.mark_passed("test", name);
    }

    fn mark_passed(&self, kind: &str, name: &str) {
        test_info!("Passed", "{kind}({name})");
        self.put_log(format_args!("#passed-{kind}({name:?})"));
    }

    fn running(&self, kind: &str, name: &str) {
        test_info!("Running", "{kind}({name})");
        self.put_log(format_args!("#running-{kind}({name:?})"));
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

    fn put_log(&self, args: fmt::Arguments) {
        use std::io::Write;
        if let Some(file) = &self.config.out_file {
            writeln!(file.lock(), "{args}").unwrap();
        }
    }

    /// Runs the tests and returns whether all tests passed.
    fn run(self) -> Result<bool> {
        self.put_log(format_args!(
            "#reset();\n#running-tests({}, {})",
            self.suites.tests.len(),
            self.suites.examples.len()
        ));
        rayon::in_place_scope(|s| {
            s.spawn(|_| {
                self.suites.tests.par_iter().for_each(|test| {
                    let name = &test.name;
                    self.running("test", name);
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
                                self.failed_test(name, format_args!("call error {err:?}"));
                            } else {
                                test_info!("Passed", "test({name})");
                                self.passed_test(name);
                            }
                        }
                        TestCaseKind::Panic => match call_once() {
                            Ok(..) => {
                                self.failed_test(name, "exited normally, expected panic");
                            }
                            Err(err) => {
                                let has_panic = err.iter().any(|p| p.message.contains("panic"));

                                if !has_panic {
                                    self.diagnostics.lock().push(err);
                                    self.failed_test(name, "exited with error, expected panic");
                                } else {
                                    test_info!("Passed", "test({name})");
                                    self.put_log(format_args!("#passed-test({name:?})"));
                                }
                            }
                        },
                        TestCaseKind::Example => {
                            let example =
                                get_example_file(&world, name, test.location, func.span());
                            match example {
                                Err(err) => {
                                    self.failed_test(name, format_args!("not found: {err}"));
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
        Ok(!self.failed.load(std::sync::atomic::Ordering::SeqCst))
    }

    fn run_example(&self, test: &Source) {
        let example_path = test.id().vpath().as_rooted_path().with_extension("");
        let example = example_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        self.running("example", example);

        if !self.examples.lock().insert(example.to_string()) {
            self.failed_example(example, "duplicate");
            return;
        }

        let world = with_main(self.world, test.id());
        let mut has_err = false;
        let doc = self.collect_diag(typst::compile::<TypstPagedDocument>(&world));
        has_err |= doc.is_none();
        if let Err(err) = self.render_paged(example, doc.as_ref()) {
            self.failed_example(example, format_args!("cannot render paged: {err}"));
            has_err = true;
        }

        if self.can_html(doc.as_ref()) {
            let doc = self.collect_diag(typst::compile::<TypstHtmlDocument>(&world));
            has_err |= doc.is_none();

            if let Err(err) = self.render_html(example, doc.as_ref()) {
                self.failed_example(example, format_args!("cannot render html: {err}"));
                has_err = true;
            }
        }

        if has_err {
            self.failed_example(example, "has error");
        } else {
            self.passed_example(example);
        }
    }

    fn render_paged(&self, example: &str, doc: Option<&TypstPagedDocument>) -> Result<()> {
        let Some(doc) = doc else {
            return Ok(());
        };
        let pixmap = typst_render::render_merged(
            doc,
            self.config.args.png.ppi / 72.0,
            Default::default(),
            None,
        );
        let output = pixmap.encode_png().context_ut("cannot encode pixmap")?;

        self.update_example(example, &output, "image")
    }

    fn render_html(&self, example: &str, doc: Option<&TypstHtmlDocument>) -> Result<()> {
        let Some(doc) = doc else {
            return Ok(());
        };
        let output = typst_html::html(doc)?.into_bytes();

        self.update_example(example, &output, "html")
    }

    fn update_example(&self, example: &str, data: &[u8], kind: &str) -> Result<()> {
        let ext = if kind == "image" { "png" } else { "html" };
        let refs_path = self.config.root.join("refs");
        let path = refs_path.join(kind).join(example).with_extension(ext);
        let tmp_path = &path.with_extension(format!("tmp.{ext}"));
        let hash_path = &path.with_extension("hash");

        let hash = &format!("siphash128_13:{:x}", tinymist_std::hash::hash128(&data));
        let existing_hash = if std::fs::exists(hash_path).context("exists hash ref")? {
            Some(std::fs::read(hash_path).context("read hash ref")?)
        } else {
            None
        };

        let equal = existing_hash.map(|existing| existing.as_slice() == hash.as_bytes());
        match (self.config.args.update, equal) {
            // Doesn't exist, create it
            (_, None) => {}
            (_, Some(true)) => {
                test_info!("Info", "example({example}): {kind} matches");
            }
            (false, Some(false)) => {
                write_atomic(tmp_path, data).context("write tmp ref")?;
                self.failed_example(
                    example,
                    format_args!("mismatch {kind} at {}", path.display()),
                );
                test_hint!(
                    "Hint",
                    "example({example}): compare {kind} at {}",
                    path.display()
                );
                match path.strip_prefix(&self.config.root) {
                    Ok(p) => {
                        self.put_log(format_args!("#mismatch-example({example:?}, {p:?})"));
                    }
                    Err(_) => {
                        self.put_log(format_args!("#mismatch-example({example:?}, none)"));
                    }
                };
                return Ok(());
            }
            (true, Some(false)) => {
                // eprintln!("   Info example({example}): updating ref {kind}");
                test_info!("Info", "example({example}): ref {kind}");
            }
        }

        if std::fs::exists(tmp_path).context("exists tmp")? {
            std::fs::remove_file(tmp_path).context("remove tmp")?;
        }

        if matches!(equal, Some(true)) {
            return Ok(());
        }

        std::fs::create_dir_all(path.parent().context("parent")?).context("create ref")?;
        write_atomic(path, data).context("write ref")?;
        write_atomic(hash_path, hash).context("write hash ref")?;

        Ok(())
    }

    fn can_html(&self, doc: Option<&TypstPagedDocument>) -> bool {
        let Some(doc) = doc else {
            return false;
        };

        let label = Label::new(PicoStr::intern("test-html-example"));
        // todo: error multiple times
        doc.introspector.query_label(label).is_ok()
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

fn print_diag_or_error<T>(world: &impl SourceWorld, result: Result<T>) -> Result<T> {
    match result {
        Ok(v) => Ok(v),
        Err(err) => {
            if let Some(diagnostics) = err.diagnostics() {
                print_diagnostics(world, diagnostics.iter(), DiagnosticFormat::Human)
                    .context_ut("print diagnostics")?;
                bail!("");
            }

            Err(err)
        }
    }
}

const PREFIX_LEN: usize = 7;

enum Level {
    Error,
    Info,
    Hint,
}

fn msg(level: Level, prefix: &str, msg: fmt::Arguments) {
    let color = match level {
        Level::Error => "\x1b[1;31m",
        Level::Info => "\x1b[1;32m",
        Level::Hint => "\x1b[1;36m",
    };
    let reset = "\x1b[0m";
    eprintln!("{color}{prefix:>PREFIX_LEN$}{reset} {msg}");
}
