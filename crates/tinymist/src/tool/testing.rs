//! Testing utilities

use core::fmt;
use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};

use itertools::Either;
use parking_lot::Mutex;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use reflexo::ImmutPath;
use reflexo_typst::{vfs::FileId, TypstDocument, TypstHtmlDocument};
use tinymist_debug::CoverageResult;
use tinymist_project::world::{system::print_diagnostics, DiagnosticFormat};
use tinymist_query::analysis::Analysis;
use tinymist_query::syntax::{cast_include_expr, find_source_by_expr, node_ancestors};
use tinymist_query::testing::{TestCaseKind, TestSuites};
use tinymist_std::{bail, error::prelude::*, fs::paths::write_atomic, typst::TypstPagedDocument};
use typst::diag::{Severity, SourceDiagnostic};
use typst::ecow::EcoVec;
use typst::foundations::{Context, Label};
use typst::syntax::{ast, LinkedNode, Source, Span};
use typst::{utils::PicoStr, World};
use typst_shim::eval::TypstEngine;

use super::project::{start_project, StartProjectResult};
use crate::world::{with_main, SourceWorld};
use crate::{project::*, utils::exit_on_ctrl_c};

const TEST_EVICT_MAX_AGE: usize = 30;
const PREFIX_LEN: usize = 7;

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

    /// Whether to render the dashboard.
    #[clap(long)]
    pub dashboard: bool,

    /// Whether not to render the dashboard.
    #[clap(long)]
    pub no_dashboard: bool,

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

    /// Whether to collect coverage.
    #[clap(long)]
    pub coverage: bool,

    /// Style of printing coverage.
    #[clap(long, default_value = "short")]
    pub print_coverage: PrintCovStyle,
}

/// Style of printing coverage.
#[derive(Debug, Clone, clap::Parser, clap::ValueEnum)]
pub enum PrintCovStyle {
    /// Don't print the coverage.
    Never,
    /// Prints the coverage in a short format.
    Short,
    /// Prints the coverage in a full format.
    Full,
}

macro_rules! test_log {
    ($level:ident,$prefix:expr, $($arg:tt)*) => {
        msg(Level::$level, $prefix, format_args!($($arg)*))
    };
}

macro_rules! test_info { ($( $arg:tt )*) => { test_log!(Info, $($arg)*) }; }
macro_rules! test_error { ($( $arg:tt )*) => { test_log!(Error, $($arg)*) }; }
macro_rules! log_info { ($( $arg:tt )*) => { test_log!(Info, "Info", $($arg)*) }; }
macro_rules! log_hint { ($( $arg:tt )*) => { test_log!(Hint, "Hint", $($arg)*) }; }

const LOG_PRELUDE: &str = "#import \"/target/testing-log.typ\": *\n#show: main";

/// Runs tests on a document
pub async fn test_main(args: TestArgs) -> Result<()> {
    exit_on_ctrl_c();

    // Prepares for the compilation
    let verse = args.compile.resolve()?;

    let root = verse.entry_state().root().map(Ok);
    let root = root
        .unwrap_or_else(|| std::env::current_dir().map(|p| p.into()))
        .context("cannot find root")?;

    std::fs::create_dir_all(Path::new("target")).context("create target dir")?;

    let dashboard = (!args.no_dashboard) && (args.dashboard || args.watch);

    let dashboard_path = "target/dashboard.typ";
    let out_file = if dashboard {
        test_info!("Info", "Dashboard is available at {dashboard_path}");
        write_atomic("target/testing-log.typ", include_str!("testing-log.typ"))
            .context("write log template")?;

        let mut out_file = std::fs::File::create(dashboard_path).context("create log file")?;
        writeln!(out_file, "{LOG_PRELUDE}").context("write log")?;
        Some(Arc::new(Mutex::new(out_file)))
    } else {
        None
    };

    let config = TestContext {
        root,
        args: args.config,
        out_file,
        analysis: Analysis::default(),
    };

    if !args.watch {
        let snap = verse.snapshot();
        return match test_once(&snap, &config) {
            Ok(true) => Ok(()),
            Ok(false) | Err(..) => std::process::exit(1),
        };
    }

    let ctx = Arc::new(Mutex::new(config.clone()));
    let repl_ctx = ctx.clone();

    let mut is_first = true;
    let StartProjectResult {
        service,
        mut editor_rx,
        intr_tx,
    } = start_project(verse, None, move |c, mut i, next| {
        if let Interrupt::Compiled(artifact) = &mut i {
            let mut config = ctx.lock();
            let instant = std::time::Instant::now();
            // todo: well term support
            // Clear the screen and then move the cursor to the top left corner.
            eprintln!("\x1B[2J\x1B[1;1H");

            if is_first {
                is_first = false;
            } else {
                log_info!("Runs testing again...");
            }
            // Sets is_compiling to track dependencies
            let mut world = artifact.snap.world.clone();
            world.set_is_compiling(true);
            let res = test_once(&world, &config);
            world.set_is_compiling(false);

            if let Err(err) = res {
                test_error!("Fatal:", "{err}");
            }
            log_info!("Tests finished in {:?}", instant.elapsed());
            if dashboard {
                log_hint!("Dashboard is available at {dashboard_path}");
            }
            log_hint!("Press 'h' for help");

            config.args.update = false;
        }

        next(c, i)
    });

    let proj_id = service.compiler.primary.id.clone();
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            std::io::stdin().read_line(&mut line).unwrap();
            match line.trim() {
                "r" => {
                    let _ = intr_tx.send(Interrupt::Compile(proj_id.clone()));
                }
                "u" => {
                    let mut repl_ctx = repl_ctx.lock();
                    repl_ctx.args.update = true;
                    let _ = intr_tx.send(Interrupt::Compile(proj_id.clone()));
                }
                "h" => eprintln!("h/r/u/c/q: help/run/update/quit"),
                "q" => std::process::exit(0),
                line => eprintln!("Unknown command: {line}"),
            }
        }
    });

    // Consume service and editor_rx
    tokio::spawn(async move { while editor_rx.recv().await.is_some() {} });

    service.run().await;

    Ok(())
}

fn test_once(world: &LspWorld, ctx: &TestContext) -> Result<bool> {
    let mut actx = ctx.analysis.enter(world.clone());
    let doc = typst::compile::<TypstPagedDocument>(&actx.world).output?;

    let suites =
        tinymist_query::testing::test_suites(&mut actx, &TypstDocument::from(Arc::new(doc)))
            .context("failed to discover tests")?;
    log_info!(
        "Found {} tests and {} examples",
        suites.tests.len(),
        suites.examples.len()
    );

    let result = if ctx.args.coverage {
        let (cov, result) = tinymist_debug::with_cov(world, |world| {
            let suites = suites.recheck(world);
            let runner = TestRunner::new(ctx, world, &suites);
            let result = print_diag_or_error(world, runner.run());
            comemo::evict(TEST_EVICT_MAX_AGE);
            result
        });
        ctx.handle_cov(world, cov?)?;
        result
    } else {
        let suites = suites.recheck(world);
        let runner = TestRunner::new(ctx, world, &suites);
        comemo::evict(TEST_EVICT_MAX_AGE);
        runner.run()
    };

    let passed = print_diag_or_error(world, result);
    if matches!(passed, Ok(true)) {
        log_info!("All test cases passed...");
    } else {
        test_error!("Fatal:", "Some test cases failed...");
    }

    passed
}

#[derive(Clone)]
struct TestContext {
    analysis: Analysis,
    root: ImmutPath,
    args: TestConfigArgs,
    out_file: Option<Arc<Mutex<std::fs::File>>>,
}

impl TestContext {
    pub fn handle_cov(&self, world: &LspWorld, cov: CoverageResult) -> Result<()> {
        let cov_path = Path::new("target/coverage.json");
        let res = serde_json::to_string(&cov.to_json(world)).context("coverage")?;
        write_atomic(cov_path, res).context("write coverage")?;
        log_info!("Written coverage to {} ...", cov_path.display());

        const COV_PREFIX: &str = "    \x1b[1;32mCov\x1b[0m ";
        match self.args.print_coverage {
            PrintCovStyle::Never => {}
            PrintCovStyle::Short => {
                eprintln!("{}", cov.summarize(true, COV_PREFIX))
            }
            PrintCovStyle::Full => {
                eprintln!("{}", cov.summarize(false, COV_PREFIX))
            }
        }
        Ok(())
    }
}

struct TestRunner<'a> {
    ctx: &'a TestContext,
    world: &'a dyn SourceWorld,
    suites: &'a TestSuites,
    diagnostics: Mutex<Vec<EcoVec<SourceDiagnostic>>>,
    examples: Mutex<HashSet<String>>,
    failed: AtomicBool,
}

impl<'a> TestRunner<'a> {
    fn new(ctx: &'a TestContext, world: &'a dyn SourceWorld, suites: &'a TestSuites) -> Self {
        Self {
            ctx,
            world,
            suites,
            diagnostics: Mutex::new(Vec::new()),
            examples: Mutex::new(HashSet::new()),
            failed: AtomicBool::new(false),
        }
    }

    fn put_log(&self, args: fmt::Arguments) {
        if let Some(file) = &self.ctx.out_file {
            writeln!(file.lock(), "{args}").unwrap();
        }
    }

    fn running(&self, kind: &str, name: &str) {
        test_info!("Running", "{kind}({name})");
        self.put_log(format_args!("#running-{kind}({name:?})"));
    }

    fn mark_failed(&self, kind: &str, name: &str, args: impl fmt::Display) {
        test_log!(Error, "Failed", "{kind}({name}): {args}");
        self.put_log(format_args!("#failed-{kind}({name:?})"));
        self.failed.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn mark_passed(&self, kind: &str, name: &str) {
        test_info!("Passed", "{kind}({name})");
        self.put_log(format_args!("#passed-{kind}({name:?})"));
    }

    fn failed_example(&self, name: &str, args: impl fmt::Display) {
        self.mark_failed("example", name, args);
    }

    fn failed_test(&self, name: &str, args: impl fmt::Display) {
        self.mark_failed("test", name, args);
    }

    /// Runs the tests and returns whether all tests passed.
    fn run(self) -> Result<bool> {
        self.put_log(format_args!(
            "#reset();\n#running-tests({}, {})",
            self.suites.tests.len(),
            self.suites.examples.len()
        ));

        let examples = self.suites.examples.par_iter().map(Either::Left);
        let tests = self.suites.tests.par_iter().map(Either::Right);

        examples.chain(tests).for_each(|case| {
            let test = match case {
                Either::Left(test) => {
                    self.run_example(test);
                    return;
                }
                Either::Right(test) => test,
            };

            let name = &test.name;
            let func = &test.function;

            let world = with_main(self.world.as_world(), test.location);
            let mut engine = TypstEngine::new(&world);

            // Executes the function
            match test.kind {
                TestCaseKind::Test | TestCaseKind::Bench => {
                    self.running("test", name);
                    if let Err(err) = engine.call(func, Context::default()) {
                        self.diagnostics.lock().push(err);
                        self.failed_test(name, format_args!("call error"));
                    } else {
                        self.mark_passed("test", name);
                    }
                }
                TestCaseKind::Panic => {
                    self.running("test", name);
                    match engine.call(func, Context::default()) {
                        Ok(..) => {
                            self.failed_test(name, "exited normally, expected panic");
                        }
                        Err(err) => {
                            let all_panic = err.iter().all(|p| p.message.contains("panic"));
                            if !all_panic {
                                self.diagnostics.lock().push(err);
                                self.failed_test(name, "exited with error, expected panic");
                            } else {
                                self.mark_passed("test", name);
                            }
                        }
                    }
                }
                TestCaseKind::Example => {
                    match get_example_file(&world, name, test.location, func.span()) {
                        Ok(example) => self.run_example(&example),
                        Err(err) => self.failed_test(name, format_args!("not found: {err}")),
                    };
                }
            }
        });

        {
            let diagnostics = self.diagnostics.into_inner();
            if !diagnostics.is_empty() {
                let diagnostics = diagnostics.into_iter().flatten().collect::<EcoVec<_>>();
                let any_error = diagnostics.iter().any(|d| d.severity == Severity::Error);

                if any_error {
                    Err(diagnostics)?
                } else {
                    print_diagnostics(self.world, diagnostics.iter(), DiagnosticFormat::Human)
                        .context_ut("print diagnostics")?;
                }
            }
        }
        Ok(!self.failed.load(std::sync::atomic::Ordering::SeqCst))
    }

    fn run_example(&self, test: &Source) {
        let id = test.id().vpath().as_rooted_path().with_extension("");
        let name = id.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        self.running("example", name);

        if !self.examples.lock().insert(name.to_string()) {
            self.failed_example(name, "duplicate");
            return;
        }

        let world = with_main(self.world.as_world(), test.id());
        let mut has_err = false;
        let (has_err_, doc) = self.build_example::<TypstPagedDocument>(&world);
        has_err |= has_err_ || self.render_paged(name, doc.as_ref());

        if self.can_html(doc.as_ref()) {
            let (has_err_, doc) = self.build_example::<TypstHtmlDocument>(&world);
            has_err |= has_err_ || self.render_html(name, doc.as_ref());
        }

        if has_err {
            self.failed_example(name, "has error");
        } else {
            self.mark_passed("example", name);
        }
    }

    fn build_example<T: typst::Document>(&self, world: &dyn World) -> (bool, Option<T>) {
        let result = typst::compile::<T>(world);
        if !result.warnings.is_empty() {
            self.diagnostics.lock().push(result.warnings);
        }

        match result.output {
            Ok(v) => (false, Some(v)),
            Err(e) => {
                self.diagnostics.lock().push(e);
                (true, None)
            }
        }
    }

    fn render_paged(&self, example: &str, doc: Option<&TypstPagedDocument>) -> bool {
        let Some(doc) = doc else {
            return false;
        };

        let ppp = self.ctx.args.png.ppi / 72.0;
        let pixmap = typst_render::render_merged(doc, ppp, Default::default(), None);
        let output = pixmap.encode_png().context_ut("cannot encode pixmap");
        let output = output.and_then(|output| self.update_example(example, &output, "paged"));
        self.check_result(example, output, "paged")
    }

    fn render_html(&self, example: &str, doc: Option<&TypstHtmlDocument>) -> bool {
        let Some(doc) = doc else {
            return false;
        };

        let output = match typst_html::html(doc) {
            Ok(output) => self.update_example(example, output.as_bytes(), "html"),
            Err(err) => {
                self.diagnostics.lock().push(err);
                Err(error_once!("render error"))
            }
        };
        self.check_result(example, output, "html")
    }

    fn check_result(&self, example: &str, res: Result<()>, kind: &str) -> bool {
        if let Err(err) = res {
            self.failed_example(example, format_args!("cannot render {kind}: {err}"));
            true
        } else {
            false
        }
    }

    fn update_example(&self, example: &str, data: &[u8], kind: &str) -> Result<()> {
        let ext = if kind == "paged" { "png" } else { "html" };
        let refs_path = self.ctx.root.join("refs");
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
        match (self.ctx.args.update, equal) {
            // Doesn't exist, create it
            (_, None) => {}
            (_, Some(true)) => log_info!("example({example}): {kind} matches"),
            (true, Some(false)) => log_info!("example({example}): ref {kind}"),
            (false, Some(false)) => {
                write_atomic(tmp_path, data).context("write tmp ref")?;

                self.failed_example(example, format_args!("mismatch {kind}"));
                log_hint!("example({example}): compare {kind} at {}", path.display());
                match path.strip_prefix(&self.ctx.root) {
                    Ok(p) => self.put_log(format_args!("#mismatch-example({example:?}, {p:?})")),
                    Err(_) => self.put_log(format_args!("#mismatch-example({example:?}, none)")),
                };

                return Ok(());
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
        cast_include_expr(name, closure.body()).context("cannot find example function")?;
    find_source_by_expr(world, id, included).context("cannot find example file")
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
