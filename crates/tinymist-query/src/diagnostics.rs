use std::borrow::Cow;

use tinymist_lint::KnownIssues;
use tinymist_world::vfs::WorkspaceResolver;
use typst::syntax::Span;

use crate::{analysis::Analysis, prelude::*};

use regex::RegexSet;

/// Stores diagnostics for files.
pub type DiagnosticsMap = HashMap<Url, EcoVec<Diagnostic>>;

type TypstDiagnostic = typst::diag::SourceDiagnostic;
type TypstSeverity = typst::diag::Severity;

/// Collects Tinymist lint diagnostics for the current compilation dependencies.
pub fn collect_lint_diagnostics<'a>(
    ctx: &mut LocalContext,
    compiler_diagnostics: impl IntoIterator<Item = &'a TypstDiagnostic>,
) -> EcoVec<TypstDiagnostic> {
    let known_issues = KnownIssues::from_compiler_diagnostics(compiler_diagnostics.into_iter());
    collect_lint_diagnostics_with_known(ctx, &known_issues)
}

fn collect_lint_diagnostics_with_known(
    ctx: &mut LocalContext,
    known_issues: &KnownIssues,
) -> EcoVec<TypstDiagnostic> {
    let mut diagnostics = EcoVec::new();
    for dep in ctx.world().depended_files() {
        if WorkspaceResolver::is_package_file(dep)
            || dep
                .vpath()
                .as_rooted_path_compat()
                .extension()
                .is_none_or(|e| e != "typ")
        {
            continue;
        }

        let Ok(source) = ctx.world().source(dep) else {
            continue;
        };

        diagnostics.extend(ctx.lint(&source, known_issues));
    }

    diagnostics
}

/// Converts a list of Typst diagnostics to LSP diagnostics,
/// with potential refinements on the error messages.
pub fn convert_diagnostics<'a>(
    graph: LspComputeGraph,
    errors: impl IntoIterator<Item = &'a TypstDiagnostic>,
    position_encoding: PositionEncoding,
) -> DiagnosticsMap {
    let analysis = Analysis {
        position_encoding,
        ..Analysis::default()
    };
    let mut ctx = analysis.enter(graph);
    DiagWorker::new(&mut ctx).convert_all(errors)
}

/// The worker for collecting diagnostics.
pub(crate) struct DiagWorker<'a> {
    /// The world surface for Typst compiler.
    pub ctx: &'a mut LocalContext,
    pub source: &'static str,
    /// Results
    pub results: DiagnosticsMap,
}

impl<'w> DiagWorker<'w> {
    /// Creates a new `CheckDocWorker` instance.
    pub fn new(ctx: &'w mut LocalContext) -> Self {
        Self {
            ctx,
            source: "typst",
            results: DiagnosticsMap::default(),
        }
    }

    /// Runs code check on the main document and all its dependencies.
    pub fn check(mut self, known_issues: &KnownIssues) -> Self {
        let source = self.source;
        self.source = "tinymist-lint";
        for diag in collect_lint_diagnostics_with_known(self.ctx, known_issues) {
            self.handle(&diag);
        }
        self.source = source;

        self
    }

    /// Converts a list of Typst diagnostics to LSP diagnostics.
    pub fn convert_all<'a>(
        mut self,
        errors: impl IntoIterator<Item = &'a TypstDiagnostic>,
    ) -> DiagnosticsMap {
        for diag in errors {
            self.handle(diag);
        }

        self.results
    }

    /// Converts a list of Typst diagnostics to LSP diagnostics.
    pub fn handle(&mut self, diag: &TypstDiagnostic) {
        match self.convert_diagnostic(diag) {
            Ok((uri, diagnostic)) => {
                self.results.entry(uri).or_default().push(diagnostic);
            }
            Err(error) => {
                log::error!("Failed to convert Typst diagnostic: {error:?}");
            }
        }
    }

    fn convert_diagnostic(
        &self,
        typst_diagnostic: &TypstDiagnostic,
    ) -> anyhow::Result<(Url, Diagnostic)> {
        let typst_diagnostic = {
            let mut diag = Cow::Borrowed(typst_diagnostic);

            // Extend more refiners here by adding their instances.
            let refiners: &[&dyn DiagnosticRefiner] =
                &[&DeprecationRefiner::<13> {}, &OutOfRootHintRefiner {}];

            // NOTE: It would be nice to have caching here.
            for refiner in refiners {
                if refiner.matches(&diag) {
                    diag = Cow::Owned(refiner.refine(diag.into_owned()));
                }
            }
            diag
        };

        let (id, span) = self.diagnostic_span_id(&typst_diagnostic);
        let uri = self.ctx.uri_for_id(id)?;
        let source = self.ctx.source_by_id(id)?;
        let lsp_range = self.diagnostic_range(&source, span);

        let lsp_severity = diagnostic_severity(typst_diagnostic.severity);
        let lsp_message = diagnostic_message(&typst_diagnostic);

        let diagnostic = Diagnostic {
            range: lsp_range,
            severity: Some(lsp_severity),
            message: lsp_message,
            source: Some(self.source.to_owned()),
            related_information: (!typst_diagnostic.trace.is_empty()).then(|| {
                typst_diagnostic
                    .trace
                    .iter()
                    .flat_map(|tracepoint| self.to_related_info(tracepoint))
                    .collect()
            }),
            ..Default::default()
        };

        Ok((uri, diagnostic))
    }

    fn to_related_info(
        &self,
        tracepoint: &Spanned<Tracepoint>,
    ) -> Option<DiagnosticRelatedInformation> {
        let id = tracepoint.span.id()?;
        // todo: expensive uri_for_id
        let uri = self.ctx.uri_for_id(id).ok()?;
        let source = self.ctx.source_by_id(id).ok()?;

        let typst_range = source_range(&source, tracepoint.span)?;
        let lsp_range = self.ctx.to_lsp_range(typst_range, &source);

        Some(DiagnosticRelatedInformation {
            location: LspLocation {
                uri,
                range: lsp_range,
            },
            message: tracepoint.v.to_string(),
        })
    }

    fn diagnostic_span_id(&self, typst_diagnostic: &TypstDiagnostic) -> (TypstFileId, DiagSpan) {
        iter::once(typst_diagnostic.span)
            .chain(typst_diagnostic.trace.iter().map(|trace| trace.span.into()))
            .find_map(|span| Some((span.id()?, span)))
            .unwrap_or_else(|| (self.ctx.world().main(), Span::detached().into()))
    }

    fn diagnostic_range(&self, source: &Source, typst_span: DiagSpan) -> LspRange {
        // Due to nvaner/typst-lsp#241 and maybe typst/typst#2035, we sometimes fail to
        // find the span. In that case, we use a default span as a better
        // alternative to panicking.
        //
        // This may have been fixed after Typst 0.7.0, but it's still nice to avoid
        // panics in case something similar reappears.
        match source_range(source, typst_span) {
            Some(range) => self.ctx.to_lsp_range(range, source),
            None => LspRange::new(LspPosition::new(0, 0), LspPosition::new(0, 0)),
        }
    }
}

fn diagnostic_severity(typst_severity: TypstSeverity) -> DiagnosticSeverity {
    match typst_severity {
        TypstSeverity::Error => DiagnosticSeverity::ERROR,
        TypstSeverity::Warning => DiagnosticSeverity::WARNING,
    }
}

fn diagnostic_message(typst_diagnostic: &TypstDiagnostic) -> String {
    let mut message = typst_diagnostic.message.to_string();
    for hint in &typst_diagnostic.hints {
        message.push_str("\nHint: ");
        message.push_str(&hint.v);
    }
    message
}

trait DiagnosticRefiner {
    fn matches(&self, raw: &TypstDiagnostic) -> bool;
    fn refine(&self, raw: TypstDiagnostic) -> TypstDiagnostic;
}

struct DeprecationRefiner<const MINOR: usize>();

static DEPRECATION_PATTERNS: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new([
        r"unknown variable: style",
        r"unexpected argument: fill",
        r"type state has no method `display`",
        r"only element functions can be used as selectors",
    ])
    .expect("Invalid regular expressions")
});

impl DiagnosticRefiner for DeprecationRefiner<13> {
    fn matches(&self, raw: &TypstDiagnostic) -> bool {
        DEPRECATION_PATTERNS.is_match(&raw.message)
    }

    fn refine(&self, raw: TypstDiagnostic) -> TypstDiagnostic {
        raw.with_hint(concat!(
            r#"Typst 0.13 has introduced breaking changes. Try downgrading "#,
            r#"Tinymist to v0.12 to use a compatible version of Typst, "#,
            r#"or consider migrating your code according to "#,
            r#"[this guide](https://typst.app/blog/2025/typst-0.13/#migrating)."#
        ))
    }
}

struct OutOfRootHintRefiner();

impl DiagnosticRefiner for OutOfRootHintRefiner {
    fn matches(&self, raw: &TypstDiagnostic) -> bool {
        raw.message.contains("failed to load file (access denied)")
            && raw
                .hints
                .iter()
                .any(|hint| hint.v.contains("cannot read file outside of project root"))
    }

    fn refine(&self, mut raw: TypstDiagnostic) -> TypstDiagnostic {
        raw.hints.clear();
        raw.with_hint("Cannot read file outside of project root.")
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use tinymist_project::{
        CompileFontArgs, DynAccessModel, EntryState, ExportTarget, LspCompileSnapshot,
        LspUniverseBuilder, base::ShadowApi, vfs::system::SystemAccessModel,
    };
    use tinymist_std::typst_shim::syntax::RootedPathExt;
    use tinymist_world::WorldComputeGraph;
    use tinymist_world::args::CompilePackageArgs;
    use typst::syntax::VirtualPath;

    use super::*;

    /// Regression test for <https://github.com/Myriad-Dreamin/tinymist/issues/2717>.
    ///
    /// Setup mirrors the reporter's layout: a workspace root plus a *local
    /// package* living outside the root, with `packagePath` configured as a
    /// *relative* path (`--package-path ../.typst/packages` style), exactly as
    /// their logs show (`could not convert path to URI: path:
    /// "../.typst/packages\..."`).
    ///
    /// The main document imports the package; evaluating the package errors.
    /// The compiler produces the error, but since the package file's resolved
    /// path is relative, `uri_for_id` fails in [`DiagWorker::convert_diagnostic`]
    /// and [`DiagWorker::handle`] only logs the failure — the diagnostic is
    /// silently dropped, so the editor shows nothing.
    ///
    /// This test asserts the *desired* behavior and therefore **fails until
    /// the root cause is fixed** (see the assertion message for the drop
    /// mechanism). The absolute-path control arm below asserts the same
    /// delivery and passes, pinning the relative `package_path` as the
    /// discriminating input.
    #[test]
    fn package_diag_dropped_with_relative_package_path_issue_2717() {
        // Relative package path, like the reporter's configuration.
        let diags = run(Path::new("../../target/issue-2717-cases/rel/pkg"), "rel");
        // #2717: currently FAILS — the package file resolves to a relative
        // path, `Url::from_file_path` rejects it, and `DiagWorker::handle`
        // swallows the conversion error, so this diagnostic never reaches the
        // editor (the reporter's Cases 2 and 3).
        assert!(
            !diags.is_empty(),
            "#2717: package diagnostics must reach the editor even with a \
             relative `package_path`; convert_diagnostics returned an empty map \
             (diagnostic dropped)"
        );
    }

    /// Control arm of the #2717 test: with an *absolute* package path the very
    /// same document reports its package error fine, which pins the relative
    /// `package_path` as the discriminating input.
    #[test]
    fn package_diag_reported_with_absolute_package_path_issue_2717() {
        let diags = run(&repo_root().join("target/issue-2717-cases/abs/pkg"), "abs");
        let messages: Vec<_> = diags
            .values()
            .flatten()
            .map(|d| d.message.as_str())
            .collect();
        assert!(
            messages.iter().any(|m| m.contains("boom")),
            "the package error must reach the editor, got: {messages:?}"
        );
    }

    /// Compiles `#import "@sisags/doc-system:0.0.1": buildDocInfo /
    /// #buildDocInfo()` where the package errors at runtime, with
    /// `package_path` set to `package_path`, and returns the LSP diagnostics
    /// the server would push to the editor.
    /// The repository root, derived from this crate's manifest location at
    /// compile time (robust to wherever the test process runs from).
    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            // `canonicalize` collapses the `..` segments; the shadow overlay
            // matches on cleaned paths only.
            .canonicalize()
            .unwrap()
    }

    fn run(package_path: &Path, tag: &str) -> DiagnosticsMap {
        let case_root = repo_root().join(format!("target/issue-2717-cases/{tag}"));
        // A relative `package_path` is interpreted against the process CWD,
        // exactly like the server does at runtime.
        let pkg_abs = std::path::absolute(package_path).unwrap();

        // Layout like the reporter's:
        //   pkg/<namespace>/<name>/<version>/{typst.toml,lib.typ}
        // i.e. the resolved package dir for `@sisags/doc-system:0.0.1`.
        let pkg_dir = pkg_abs.join("sisags/doc-system/0.0.1");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("typst.toml"),
            "[package]\nname = \"doc-system\"\nversion = \"0.0.1\"\nentrypoint = \"lib.typ\"\n",
        )
        .unwrap();
        // Runtime error inside the package (like the reporter's
        // `buildDocInfo.typ`).
        std::fs::write(
            pkg_dir.join("lib.typ"),
            "#let buildDocInfo() = panic(\"boom\")\n",
        )
        .unwrap();

        // Workspace root (absolute, canonical) holding only `main.typ`.
        let proj = case_root.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let root = proj;

        let font_resolver = Arc::new(
            LspUniverseBuilder::resolve_fonts(CompileFontArgs {
                ignore_system_fonts: true,
                ..Default::default()
            })
            .unwrap(),
        );
        // The key bit of the buggy arm: the local package path is *relative*,
        // like the reporter's configuration.
        let registry = LspUniverseBuilder::resolve_package(
            None,
            Some(&CompilePackageArgs {
                package_path: Some(PathBuf::from(package_path)),
                package_cache_path: None,
            }),
        );
        let mut verse = LspUniverseBuilder::build(
            EntryState::new_rooted(
                root.as_path().into(),
                Some(VirtualPath::new("main.typ").unwrap()),
            ),
            ExportTarget::Paged,
            Default::default(),
            Default::default(),
            registry,
            font_resolver,
            None,
            DynAccessModel(Arc::new(SystemAccessModel {})),
        );
        verse
            .map_shadow(
                root.join("main.typ").as_path(),
                typst::foundations::Bytes::from_string(
                    "#import \"@sisags/doc-system:0.0.1\": buildDocInfo\n#buildDocInfo()\n"
                        .to_owned(),
                ),
            )
            .unwrap();

        // Compile the world, as the server would on a preview request.
        let mut snap = LspCompileSnapshot::from_world(verse.snapshot());
        snap.world.set_is_compiling(true);
        let compiled = typst_shim::compile_opt::<typst_layout::PagedDocument>(&snap.world);
        assert!(
            compiled.output.is_err(),
            "the document must fail to compile (error inside the package)"
        );
        let mut diags = compiled.warnings.clone();
        if let Err(errors) = &compiled.output {
            diags.extend(errors.iter().cloned());
        }
        assert_eq!(diags.len(), 1, "exactly one error must be produced");

        // The error's span must indeed point into the package file — the same
        // shape as the reporter's failing diagnostic.
        let span_pkg = diags[0]
            .span
            .id()
            .and_then(|id| id.package_compat().cloned())
            .expect("the error span must live in the package file");
        assert_eq!(span_pkg.namespace.as_str(), "sisags");

        // What the server does before pushing to the editor:
        let graph = WorldComputeGraph::new(snap);
        let map = convert_diagnostics(graph, diags.iter(), PositionEncoding::Utf16);

        // Cleanup (best effort).
        let _ = std::fs::remove_dir_all(&case_root);

        map
    }
}
