use ecow::EcoString;

use std::io::IsTerminal;
use std::str::FromStr;

use codespan_reporting::term::termcolor::{ColorChoice, NoColor, StandardStream, WriteColor};
use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    term,
};
use tinymist_std::Result;
use tinymist_vfs::FileId;
use typst::diag::{eco_format, Severity, SourceDiagnostic, StrResult};
use typst::syntax::Span;

use crate::{CodeSpanReportWorld, DiagnosticFormat, SourceWorld};

/// Get stderr with color support if desirable.
fn color_stream() -> StandardStream {
    StandardStream::stderr(if std::io::stderr().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    })
}

/// Print diagnostic messages to the terminal.
pub fn print_diagnostics<'d, 'files>(
    world: &'files dyn SourceWorld,
    errors: impl Iterator<Item = &'d SourceDiagnostic>,
    diagnostic_format: DiagnosticFormat,
) -> Result<(), codespan_reporting::files::Error> {
    let mut w = match diagnostic_format {
        DiagnosticFormat::Human => color_stream(),
        DiagnosticFormat::Short => StandardStream::stderr(ColorChoice::Never),
    };

    print_diagnostics_to(world, errors, &mut w, diagnostic_format)
}

/// Print diagnostic messages to the terminal.
pub fn print_diagnostics_to_string<'d, 'files>(
    world: &'files dyn SourceWorld,
    errors: impl Iterator<Item = &'d SourceDiagnostic>,
    diagnostic_format: DiagnosticFormat,
) -> StrResult<EcoString> {
    let mut w = NoColor::new(vec![]);

    print_diagnostics_to(world, errors, &mut w, diagnostic_format)
        .map_err(|e| eco_format!("failed to print diagnostics to string: {e}"))?;
    let output = EcoString::from_str(
        std::str::from_utf8(&w.into_inner())
            .map_err(|e| eco_format!("failed to convert diagnostics to string: {e}"))?,
    )
    .unwrap_or_default();
    Ok(output)
}

/// Print diagnostic messages to the terminal.
pub fn print_diagnostics_to<'d, 'files>(
    world: &'files dyn SourceWorld,
    errors: impl Iterator<Item = &'d SourceDiagnostic>,
    w: &mut impl WriteColor,
    diagnostic_format: DiagnosticFormat,
) -> Result<(), codespan_reporting::files::Error> {
    let world = CodeSpanReportWorld::new(world);

    let mut config = term::Config {
        tab_width: 2,
        ..Default::default()
    };
    if diagnostic_format == DiagnosticFormat::Short {
        config.display_style = term::DisplayStyle::Short;
    }

    for diagnostic in errors {
        let diag = match diagnostic.severity {
            Severity::Error => Diagnostic::error(),
            Severity::Warning => Diagnostic::warning(),
        }
        .with_message(diagnostic.message.clone())
        .with_notes(
            diagnostic
                .hints
                .iter()
                .map(|e| (eco_format!("hint: {e}")).into())
                .collect(),
        )
        .with_labels(label(world.world, diagnostic.span).into_iter().collect());

        term::emit(w, &config, &world, &diag)?;

        // Stacktrace-like helper diagnostics.
        for point in &diagnostic.trace {
            let message = point.v.to_string();
            let help = Diagnostic::help()
                .with_message(message)
                .with_labels(label(world.world, point.span).into_iter().collect());

            term::emit(w, &config, &world, &help)?;
        }
    }

    Ok(())
}

/// Create a label for a span.
fn label(world: &dyn SourceWorld, span: Span) -> Option<Label<FileId>> {
    Some(Label::primary(span.id()?, world.source_range(span)?))
}
