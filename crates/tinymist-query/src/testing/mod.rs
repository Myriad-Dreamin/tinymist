//! Extracts test suites from the document.

use ecow::EcoString;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use tinymist_std::error::prelude::*;
use tinymist_std::typst::TypstDocument;
use tinymist_world::vfs::FileId;
use typst::{
    foundations::{Func, Label, Module, Selector, Value},
    introspection::MetadataElem,
    syntax::Source,
    utils::PicoStr,
    World,
};

use crate::LocalContext;

/// Test suites extracted from the document.
pub struct TestSuites {
    /// Files from the current workspace.
    pub origin_files: Vec<(Source, Module)>,
    /// Test cases in the current workspace.
    pub tests: Vec<TestCase>,
    /// Example documents in the current workspace.
    pub examples: Vec<Source>,
}
impl TestSuites {
    /// Rechecks the test suites.
    pub fn recheck(&self, world: &dyn World) -> TestSuites {
        let tests = self
            .tests
            .iter()
            .filter_map(|test| {
                let source = world.source(test.location).ok()?;
                let module = typst_shim::eval::eval_compat(world, &source).ok()?;
                let symbol = module.scope().get(&test.name)?;
                let Value::Func(function) = symbol.read() else {
                    return None;
                };
                Some(TestCase {
                    name: test.name.clone(),
                    location: test.location,
                    function: function.clone(),
                    kind: test.kind,
                })
            })
            .collect();

        let examples = self
            .examples
            .iter()
            .filter_map(|source| world.source(source.id()).ok())
            .collect();

        TestSuites {
            origin_files: self.origin_files.clone(),
            tests,
            examples,
        }
    }
}

/// Kind of the test case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestCaseKind {
    /// A normal test case.
    Test,
    /// A test case that should panic.
    Panic,
    /// A benchmark test case.
    Bench,
    /// An example test case.
    Example,
}

/// A test case.
pub struct TestCase {
    /// Name of the test case.
    pub name: EcoString,
    /// Location of the test case.
    pub location: FileId,
    /// entry of the test case.
    pub function: Func,
    /// Kind of the test case.
    pub kind: TestCaseKind,
}

/// Extracts the test suites in the document
pub fn test_suites(ctx: &mut LocalContext, doc: &TypstDocument) -> Result<TestSuites> {
    let main_id = ctx.world.main();
    let main_workspace = main_id.package();

    crate::log_debug_ct!(
        "test workspace: {:?}, files: {:?}",
        main_workspace,
        ctx.depended_source_files()
    );
    let files = ctx
        .depended_source_files()
        .par_iter()
        .filter(|fid| fid.package() == main_workspace)
        .map(|fid| {
            let source = ctx
                .source_by_id(*fid)
                .context_ut("failed to get source by id")?;
            let module = ctx.module_by_id(*fid)?;
            Ok((source, module))
        })
        .collect::<Result<Vec<_>>>()?;

    let config = extract_test_configuration(doc)?;

    let mut worker = TestSuitesWorker {
        files: &files,
        config,
        tests: Vec::new(),
        examples: Vec::new(),
    };

    worker.discover_tests()?;

    Ok(TestSuites {
        tests: worker.tests,
        examples: worker.examples,
        origin_files: files,
    })
}

#[derive(Debug, Clone)]
struct TestConfig {
    test_pattern: EcoString,
    bench_pattern: EcoString,
    panic_pattern: EcoString,
    example_pattern: EcoString,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct UserTestConfig {
    test_pattern: Option<EcoString>,
    bench_pattern: Option<EcoString>,
    panic_pattern: Option<EcoString>,
    example_pattern: Option<EcoString>,
}

fn extract_test_configuration(doc: &TypstDocument) -> Result<TestConfig> {
    let selector = Label::new(PicoStr::intern("test-config"));
    let metadata = doc.introspector().query(&Selector::Label(selector));
    if metadata.len() > 1 {
        // todo: attach source locations.
        bail!("multiple test configurations found");
    }

    let config = if let Some(metadata) = metadata.first() {
        let metadata = metadata
            .to_packed::<MetadataElem>()
            .context("test configuration is not a metadata element")?;

        let value =
            serde_json::to_value(&metadata.value).context("failed to serialize metadata")?;
        serde_json::from_value(value).context("failed to deserialize metadata")?
    } else {
        UserTestConfig::default()
    };

    Ok(TestConfig {
        test_pattern: config.test_pattern.unwrap_or_else(|| "test-".into()),
        bench_pattern: config.bench_pattern.unwrap_or_else(|| "bench-".into()),
        panic_pattern: config.panic_pattern.unwrap_or_else(|| "panic-on-".into()),
        example_pattern: config.example_pattern.unwrap_or_else(|| "example-".into()),
    })
}

struct TestSuitesWorker<'a> {
    files: &'a [(Source, Module)],
    config: TestConfig,
    tests: Vec<TestCase>,
    examples: Vec<Source>,
}

impl TestSuitesWorker<'_> {
    fn match_test(&self, name: &str) -> Option<TestCaseKind> {
        if name.starts_with(self.config.test_pattern.as_str()) {
            Some(TestCaseKind::Test)
        } else if name.starts_with(self.config.bench_pattern.as_str()) {
            Some(TestCaseKind::Bench)
        } else if name.starts_with(self.config.panic_pattern.as_str()) {
            Some(TestCaseKind::Panic)
        } else if name.starts_with(self.config.example_pattern.as_str()) {
            Some(TestCaseKind::Example)
        } else {
            None
        }
    }

    fn discover_tests(&mut self) -> Result<()> {
        for (source, module) in self.files.iter() {
            let vpath = source.id().vpath().as_rooted_path();
            let file_name = vpath.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if file_name.starts_with(self.config.example_pattern.as_str()) {
                self.examples.push(source.clone());
                continue;
            }

            for (name, symbol) in module.scope().iter() {
                crate::log_debug_ct!("symbol({name:?}): {symbol:?}");
                let Value::Func(function) = symbol.read() else {
                    continue;
                };

                let span = symbol.span();
                let id = span.id();
                if Some(source.id()) != id {
                    continue;
                }

                if let Some(kind) = self.match_test(name.as_str()) {
                    self.tests.push(TestCase {
                        name: name.clone(),
                        location: source.id(),
                        function: function.clone(),
                        kind,
                    });
                }
            }
        }

        Ok(())
    }
}
