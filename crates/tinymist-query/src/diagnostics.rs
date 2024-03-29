use crate::{path_to_url, prelude::*};

/// Stores diagnostics for files.
pub type DiagnosticsMap = HashMap<Url, Vec<LspDiagnostic>>;

/// Converts a list of Typst diagnostics to LSP diagnostics.
pub fn convert_diagnostics<'a>(
    ctx: &AnalysisContext,
    errors: impl IntoIterator<Item = &'a TypstDiagnostic>,
) -> DiagnosticsMap {
    errors
        .into_iter()
        .flat_map(|error| {
            convert_diagnostic(ctx, error)
                .map_err(move |conversion_err| {
                    error!("could not convert Typst error to diagnostic: {conversion_err:?} error to convert: {error:?}");
                })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .into_group_map()
}

fn convert_diagnostic(
    ctx: &AnalysisContext,
    typst_diagnostic: &TypstDiagnostic,
) -> anyhow::Result<(Url, LspDiagnostic)> {
    let uri;
    let lsp_range;
    if let Some((id, span)) = diagnostic_span_id(typst_diagnostic) {
        uri = path_to_url(&ctx.path_for_id(id)?)?;
        let source = ctx.world().source(id)?;
        lsp_range = diagnostic_range(&source, span, ctx.position_encoding());
    } else {
        uri = path_to_url(&ctx.analysis.root)?;
        lsp_range = LspRange::default();
    };

    let lsp_severity = diagnostic_severity(typst_diagnostic.severity);

    let typst_message = &typst_diagnostic.message;
    let typst_hints = &typst_diagnostic.hints;
    let lsp_message = format!("{typst_message}{}", diagnostic_hints(typst_hints));

    let tracepoints =
        diagnostic_related_information(ctx, typst_diagnostic, ctx.position_encoding())?;

    let diagnostic = LspDiagnostic {
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
    project: &AnalysisContext,
    tracepoint: &Spanned<Tracepoint>,
    position_encoding: PositionEncoding,
) -> anyhow::Result<Option<DiagnosticRelatedInformation>> {
    if let Some(id) = tracepoint.span.id() {
        let uri = path_to_url(&project.path_for_id(id)?)?;
        let source = project.world().source(id)?;

        if let Some(typst_range) = source.range(tracepoint.span) {
            let lsp_range = typst_to_lsp::range(typst_range, &source, position_encoding);

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
    project: &AnalysisContext,
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

fn diagnostic_span_id(typst_diagnostic: &TypstDiagnostic) -> Option<(TypstFileId, TypstSpan)> {
    iter::once(typst_diagnostic.span)
        .chain(typst_diagnostic.trace.iter().map(|trace| trace.span))
        .find_map(|span| Some((span.id()?, span)))
}

fn diagnostic_range(
    source: &Source,
    typst_span: TypstSpan,
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
            typst_to_lsp::range(typst_range, source, position_encoding)
        }
        None => LspRange::new(LspPosition::new(0, 0), LspPosition::new(0, 0)),
    }
}

fn diagnostic_severity(typst_severity: TypstSeverity) -> LspSeverity {
    match typst_severity {
        TypstSeverity::Error => LspSeverity::ERROR,
        TypstSeverity::Warning => LspSeverity::WARNING,
    }
}

fn diagnostic_hints(typst_hints: &[EcoString]) -> Format<impl Iterator<Item = EcoString> + '_> {
    iter::repeat(EcoString::from("\n\nHint: "))
        .take(typst_hints.len())
        .interleave(typst_hints.iter().cloned())
        .format("")
}
