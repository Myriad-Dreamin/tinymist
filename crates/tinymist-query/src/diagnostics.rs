use std::borrow::Cow;

use tinymist_project::LspWorld;
use typst::syntax::Span;

use crate::{prelude::*, LspWorldExt};

/// Stores diagnostics for files.
pub type DiagnosticsMap = HashMap<Url, EcoVec<Diagnostic>>;

type TypstDiagnostic = typst::diag::SourceDiagnostic;
type TypstSeverity = typst::diag::Severity;

/// Context for converting Typst diagnostics to LSP diagnostics.
struct LocalDiagContext<'a> {
    /// The world surface for Typst compiler.
    pub world: &'a LspWorld,
    /// The position encoding for the source.
    pub position_encoding: PositionEncoding,
}

impl std::ops::Deref for LocalDiagContext<'_> {
    type Target = LspWorld;

    fn deref(&self) -> &Self::Target {
        self.world
    }
}

/// Converts a list of Typst diagnostics to LSP diagnostics,
/// with potential refinements on the error messages.
pub fn convert_diagnostics<'a>(
    world: &LspWorld,
    errors: impl IntoIterator<Item = &'a TypstDiagnostic>,
    position_encoding: PositionEncoding,
) -> DiagnosticsMap {
    let ctx = LocalDiagContext {
        world,
        position_encoding,
    };

    let kvs = errors
        .into_iter()
        .flat_map(|error| {
            convert_diagnostic(&ctx, error)
                .map_err(move |conversion_err| {
                    log::error!("could not convert Typst error to diagnostic: {conversion_err:?} error to convert: {error:?}");
                })
        });

    let mut lookup = HashMap::new();
    for (key, val) in kvs {
        lookup.entry(key).or_insert_with(EcoVec::new).push(val);
    }

    lookup
}

fn convert_diagnostic(
    ctx: &LocalDiagContext,
    typst_diagnostic: &TypstDiagnostic,
) -> anyhow::Result<(Url, Diagnostic)> {
    let typst_diagnostic = {
        let mut diag = Cow::Borrowed(typst_diagnostic);

        // Extend more refiners here by adding their instances.
        let refiners = &[&DeprecationRefiner::<13> {}];

        // NOTE: It would be nice to have caching here.
        for refiner in refiners {
            if refiner.matches(&diag) {
                diag = Cow::Owned(refiner.refine(diag.into_owned()));
            }
        }
        diag
    };

    let (id, span) = diagnostic_span_id(ctx, &typst_diagnostic);
    let uri = ctx.uri_for_id(id)?;
    let source = ctx.source(id)?;
    let lsp_range = diagnostic_range(&source, span, ctx.position_encoding);

    let lsp_severity = diagnostic_severity(typst_diagnostic.severity);

    let typst_message = &typst_diagnostic.message;
    let typst_hints = &typst_diagnostic.hints;
    let lsp_message = format!("{typst_message}{}", diagnostic_hints(typst_hints));

    let tracepoints =
        diagnostic_related_information(ctx, &typst_diagnostic, ctx.position_encoding)?;

    let diagnostic = Diagnostic {
        range: lsp_range,
        severity: Some(lsp_severity),
        message: lsp_message,
        source: Some("typst".to_owned()),
        related_information: Some(tracepoints),
        ..Default::default()
    };

    Ok((uri, diagnostic))
}

fn tracepoint_to_relatedinformation(
    ctx: &LocalDiagContext,
    tracepoint: &Spanned<Tracepoint>,
    position_encoding: PositionEncoding,
) -> anyhow::Result<Option<DiagnosticRelatedInformation>> {
    if let Some(id) = tracepoint.span.id() {
        let uri = ctx.uri_for_id(id)?;
        let source = ctx.source(id)?;

        if let Some(typst_range) = source.range(tracepoint.span) {
            let lsp_range = to_lsp_range(typst_range, &source, position_encoding);

            return Ok(Some(DiagnosticRelatedInformation {
                location: LspLocation {
                    uri,
                    range: lsp_range,
                },
                message: tracepoint.v.to_string(),
            }));
        }
    }

    Ok(None)
}

fn diagnostic_related_information(
    project: &LocalDiagContext,
    typst_diagnostic: &TypstDiagnostic,
    position_encoding: PositionEncoding,
) -> anyhow::Result<Vec<DiagnosticRelatedInformation>> {
    let mut tracepoints = vec![];

    for tracepoint in &typst_diagnostic.trace {
        if let Some(info) =
            tracepoint_to_relatedinformation(project, tracepoint, position_encoding)?
        {
            tracepoints.push(info);
        }
    }

    Ok(tracepoints)
}

fn diagnostic_span_id(
    ctx: &LocalDiagContext,
    typst_diagnostic: &TypstDiagnostic,
) -> (TypstFileId, Span) {
    iter::once(typst_diagnostic.span)
        .chain(typst_diagnostic.trace.iter().map(|trace| trace.span))
        .find_map(|span| Some((span.id()?, span)))
        .unwrap_or_else(|| (ctx.main(), Span::detached()))
}

fn diagnostic_range(
    source: &Source,
    typst_span: Span,
    position_encoding: PositionEncoding,
) -> LspRange {
    // Due to nvaner/typst-lsp#241 and maybe typst/typst#2035, we sometimes fail to
    // find the span. In that case, we use a default span as a better
    // alternative to panicking.
    //
    // This may have been fixed after Typst 0.7.0, but it's still nice to avoid
    // panics in case something similar reappears.
    match source.find(typst_span) {
        Some(node) => {
            let typst_range = node.range();
            to_lsp_range(typst_range, source, position_encoding)
        }
        None => LspRange::new(LspPosition::new(0, 0), LspPosition::new(0, 0)),
    }
}

fn diagnostic_severity(typst_severity: TypstSeverity) -> DiagnosticSeverity {
    match typst_severity {
        TypstSeverity::Error => DiagnosticSeverity::ERROR,
        TypstSeverity::Warning => DiagnosticSeverity::WARNING,
    }
}

fn diagnostic_hints(typst_hints: &[EcoString]) -> Format<impl Iterator<Item = EcoString> + '_> {
    iter::repeat(EcoString::from("\n\nHint: "))
        .take(typst_hints.len())
        .interleave(typst_hints.iter().cloned())
        .format("")
}

trait DiagnosticRefiner {
    fn matches(&self, raw: &TypstDiagnostic) -> bool;
    fn refine(&self, raw: TypstDiagnostic) -> TypstDiagnostic;
}

struct DeprecationRefiner<const MINOR: usize>();

impl DiagnosticRefiner for DeprecationRefiner<13> {
    fn matches(&self, raw: &TypstDiagnostic) -> bool {
        let msg = &raw.message;
        let patterns = [
            "unknown variable: style",
            "unexpected argument: fill",
            "type state has no method `display`",
            "only element functions can be used as selectors",
        ];

        patterns.iter().any(|pattern| msg.contains(pattern))
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
