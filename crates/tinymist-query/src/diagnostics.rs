use std::borrow::Cow;

use lsp_types::NumberOrString;
use tinymist_lint::{KnownIssues, LintDiagnostic, LintLevel, LintMetadata, LintRule};
use tinymist_world::vfs::WorkspaceResolver;
use typst::syntax::Span;

use crate::{analysis::Analysis, prelude::*};

use regex::RegexSet;

/// Stores diagnostics for files.
pub type DiagnosticsMap = HashMap<Url, EcoVec<Diagnostic>>;

type TypstDiagnostic = typst::diag::SourceDiagnostic;
type TypstSeverity = typst::diag::Severity;

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
                self.handle_lint(&diag);
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

    /// Extends the worker with lint diagnostics.
    pub fn push_lints<'a>(&mut self, diagnostics: impl IntoIterator<Item = &'a LintDiagnostic>) {
        let source = self.source;
        self.source = "tinymist-lint";
        for diag in diagnostics {
            self.handle_lint(diag);
        }
        self.source = source;
    }

    /// Converts a list of lint diagnostics to LSP diagnostics. (tests only)
    #[cfg(test)]
    pub fn convert_lints<'a>(
        mut self,
        diagnostics: impl IntoIterator<Item = &'a LintDiagnostic>,
    ) -> DiagnosticsMap {
        self.push_lints(diagnostics);
        self.results
    }

    /// Converts a list of Typst diagnostics to LSP diagnostics.
    pub fn handle(&mut self, diag: &TypstDiagnostic) {
        match self.convert_diagnostic(diag, None, None, None) {
            Ok((uri, diagnostic)) => {
                self.results.entry(uri).or_default().push(diagnostic);
            }
            Err(error) => {
                log::error!("Failed to convert Typst diagnostic: {error:?}");
            }
        }
    }

    fn handle_lint(&mut self, diag: &LintDiagnostic) {
        match self.convert_lint_diagnostic(diag) {
            Ok(Some((uri, diagnostic))) => {
                self.results.entry(uri).or_default().push(diagnostic);
            }
            Ok(None) => {}
            Err(error) => {
                log::error!("Failed to convert lint diagnostic: {error:?}");
            }
        }
    }

    fn convert_lint_diagnostic(
        &self,
        lint_diagnostic: &LintDiagnostic,
    ) -> anyhow::Result<Option<(Url, Diagnostic)>> {
        let level = self.ctx.analysis.lint_settings.level(lint_diagnostic.rule);
        if matches!(level, LintLevel::Off) {
            return Ok(None);
        }
        let severity = lint_level_to_severity(level);
        self.convert_diagnostic(
            &lint_diagnostic.diagnostic,
            Some(lint_diagnostic.rule),
            Some(severity),
            lint_diagnostic.metadata(),
        )
        .map(Some)
    }

    fn convert_diagnostic(
        &self,
        typst_diagnostic: &TypstDiagnostic,
        lint_rule: Option<LintRule>,
        severity_override: Option<DiagnosticSeverity>,
        lint_metadata: Option<LintMetadata>,
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

        let typst_kind = classify_typst_diagnostic(&typst_diagnostic.message);
        let lsp_severity =
            severity_override.unwrap_or_else(|| diagnostic_severity(&typst_diagnostic, typst_kind));
        let lsp_message = diagnostic_message(&typst_diagnostic);
        let is_unused = matches!(lint_rule, Some(LintRule::DeadCode))
            || matches!(typst_kind, Some(TypstDiagKind::Unused));
        let code = lint_rule
            .map(|rule| NumberOrString::String(rule.code().into()))
            .or_else(|| typst_kind.map(|kind| NumberOrString::String(kind.code().into())));
        let data = lint_metadata.and_then(|meta| serde_json::to_value(meta).ok());

        let diagnostic = Diagnostic {
            range: lsp_range,
            severity: Some(lsp_severity),
            message: lsp_message,
            source: Some(self.source.to_owned()),
            code,
            tags: is_unused.then(|| vec![DiagnosticTag::UNNECESSARY]),
            data,
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
}

fn diagnostic_severity(
    typst_diagnostic: &TypstDiagnostic,
    kind: Option<TypstDiagKind>,
) -> DiagnosticSeverity {
    if matches!(kind, Some(TypstDiagKind::Unused)) {
        return DiagnosticSeverity::HINT;
    }

    match typst_diagnostic.severity {
        TypstSeverity::Error => DiagnosticSeverity::ERROR,
        TypstSeverity::Warning => DiagnosticSeverity::WARNING,
    }
}

fn lint_level_to_severity(level: LintLevel) -> DiagnosticSeverity {
    match level {
        LintLevel::Error => DiagnosticSeverity::ERROR,
        LintLevel::Warning => DiagnosticSeverity::WARNING,
        LintLevel::Info => DiagnosticSeverity::INFORMATION,
        LintLevel::Hint => DiagnosticSeverity::HINT,
        LintLevel::Off => unreachable!("disabled lints should be filtered early"),
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

fn classify_typst_diagnostic(message: &str) -> Option<TypstDiagKind> {
    if message.starts_with("unknown variable") {
        Some(TypstDiagKind::UnknownVariable)
    } else if message.starts_with("file not found") {
        Some(TypstDiagKind::FileNotFound)
    } else if message.starts_with("unused ") {
        Some(TypstDiagKind::Unused)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypstDiagKind {
    UnknownVariable,
    FileNotFound,
    Unused,
}

impl TypstDiagKind {
    fn code(self) -> &'static str {
        match self {
            TypstDiagKind::UnknownVariable => "typst.unknown-variable",
            TypstDiagKind::FileNotFound => "typst.file-not-found",
            TypstDiagKind::Unused => "typst.unused",
        }
    }
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
