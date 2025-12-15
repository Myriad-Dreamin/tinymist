use std::borrow::Cow;

use tinymist_lint::{DOCUMENTED_EXPORTED_FUNCTION_HINT, KnownIssues};
use tinymist_world::vfs::WorkspaceResolver;
use typst::syntax::Span;

use crate::{analysis::Analysis, prelude::*};

use regex::RegexSet;

/// Stores diagnostics for files.
pub type DiagnosticsMap = HashMap<Url, EcoVec<Diagnostic>>;

type TypstDiagnostic = typst::diag::SourceDiagnostic;
type TypstSeverity = typst::diag::Severity;

const UNUSED_CODE_IMPORTED_ITEM: &str = "tinymist.unused.import";
const UNUSED_CODE_MODULE_IMPORT: &str = "tinymist.unused.module_import";
const UNUSED_CODE_MODULE: &str = "tinymist.unused.module";
const UNUSED_CODE_SYMBOL: &str = "tinymist.unused.symbol";
const UNUSED_CODE_EXPORTED_DOCUMENTED_FUNCTION: &str =
    "tinymist.unused.exported_documented_function";

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
        for dep in self.ctx.world().depended_files() {
            if WorkspaceResolver::is_package_file(dep) {
                continue;
            }

            let Ok(source) = self.ctx.world().source(dep) else {
                continue;
            };

            for diag in self.ctx.lint(&source, known_issues) {
                self.handle(&diag);
            }
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

        let lsp_severity = diagnostic_severity(&typst_diagnostic);
        let lsp_message = diagnostic_message(&typst_diagnostic);
        let is_unused = typst_diagnostic.message.starts_with("unused ");
        let unused_code = is_unused
            .then(|| self.unused_code(&typst_diagnostic))
            .flatten();
        let is_documented_exported_function =
            unused_code.is_some_and(|code| code == UNUSED_CODE_EXPORTED_DOCUMENTED_FUNCTION);

        let diagnostic = Diagnostic {
            range: lsp_range,
            severity: Some(lsp_severity),
            message: lsp_message,
            source: Some(self.source.to_owned()),
            code: unused_code.map(|code| lsp_types::NumberOrString::String(code.to_string())),
            tags: (is_unused && !is_documented_exported_function)
                .then(|| vec![DiagnosticTag::UNNECESSARY]),
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

        let typst_range = source.range(tracepoint.span)?;
        let lsp_range = self.ctx.to_lsp_range(typst_range, &source);

        Some(DiagnosticRelatedInformation {
            location: LspLocation {
                uri,
                range: lsp_range,
            },
            message: tracepoint.v.to_string(),
        })
    }

    fn diagnostic_span_id(&self, typst_diagnostic: &TypstDiagnostic) -> (TypstFileId, Span) {
        iter::once(typst_diagnostic.span)
            .chain(typst_diagnostic.trace.iter().map(|trace| trace.span))
            .find_map(|span| Some((span.id()?, span)))
            .unwrap_or_else(|| (self.ctx.world().main(), Span::detached()))
    }

    fn diagnostic_range(&self, source: &Source, typst_span: Span) -> LspRange {
        // Due to nvaner/typst-lsp#241 and maybe typst/typst#2035, we sometimes fail to
        // find the span. In that case, we use a default span as a better
        // alternative to panicking.
        //
        // This may have been fixed after Typst 0.7.0, but it's still nice to avoid
        // panics in case something similar reappears.
        match source.find(typst_span) {
            Some(node) => self.ctx.to_lsp_range(node.range(), source),
            None => LspRange::new(LspPosition::new(0, 0), LspPosition::new(0, 0)),
        }
    }

    fn unused_code(&self, diag: &TypstDiagnostic) -> Option<&'static str> {
        let msg = diag.message.as_str();
        if msg.starts_with("unused import:") {
            return Some(UNUSED_CODE_IMPORTED_ITEM);
        }
        if msg.starts_with("unused module import") {
            return Some(UNUSED_CODE_MODULE_IMPORT);
        }
        if msg.starts_with("unused module:") {
            return Some(UNUSED_CODE_MODULE);
        }
        if msg.starts_with("unused function:")
            && diag
                .hints
                .iter()
                .any(|hint| hint.as_str() == DOCUMENTED_EXPORTED_FUNCTION_HINT)
        {
            return Some(UNUSED_CODE_EXPORTED_DOCUMENTED_FUNCTION);
        }
        if msg.starts_with("unused ") {
            return Some(UNUSED_CODE_SYMBOL);
        }
        None
    }
}

fn diagnostic_severity(typst_diagnostic: &TypstDiagnostic) -> DiagnosticSeverity {
    if typst_diagnostic.message.starts_with("unused ") {
        return DiagnosticSeverity::HINT;
    }

    match typst_diagnostic.severity {
        TypstSeverity::Error => DiagnosticSeverity::ERROR,
        TypstSeverity::Warning => DiagnosticSeverity::WARNING,
    }
}

fn diagnostic_message(typst_diagnostic: &TypstDiagnostic) -> String {
    let mut message = typst_diagnostic.message.to_string();
    for hint in &typst_diagnostic.hints {
        message.push_str("\nHint: ");
        message.push_str(hint);
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
                .any(|hint| hint.contains("cannot read file outside of project root"))
    }

    fn refine(&self, mut raw: TypstDiagnostic) -> TypstDiagnostic {
        raw.hints.clear();
        raw.with_hint("Cannot read file outside of project root.")
    }
}
