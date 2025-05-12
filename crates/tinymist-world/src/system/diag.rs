use std::io::IsTerminal;

use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    term::{
        self,
        termcolor::{ColorChoice, StandardStream},
    },
};
use tinymist_std::Result;
use tinymist_vfs::FileId;
use typst::diag::{eco_format, Severity, SourceDiagnostic, SourceResult, Warned};
use typst::syntax::Span;
use typst::WorldExt;

use crate::{CodeSpanReportWorld, DiagnosticFormat, SourceWorld};

/// Get stderr with color support if desirable.
fn color_stream() -> StandardStream {
    StandardStream::stderr(if std::io::stderr().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    })
}

/// Prints compilation diagnostic messages to the terminal.
pub fn print_compile_diagnostics<T>(
    world: &dyn SourceWorld,
    result: Warned<SourceResult<T>>,
    diagnostic_format: DiagnosticFormat,
) -> Result<Option<T>, codespan_reporting::files::Error> {
    match result.output {
        Ok(value) => {
            if !result.warnings.is_empty() {
                print_diagnostics(world, result.warnings.iter(), diagnostic_format)?;
            }

            Ok(Some(value))
        }
        Err(errors) => {
            let diag = errors.iter().chain(result.warnings.iter());

            print_diagnostics(world, diag, diagnostic_format)?;
            Ok(None)
        }
    }
}

/// Prints diagnostic messages to the terminal.
pub fn print_diagnostics<'d, 'files>(
    world: &'files dyn SourceWorld,
    errors: impl Iterator<Item = &'d SourceDiagnostic>,
    diagnostic_format: DiagnosticFormat,
) -> Result<(), codespan_reporting::files::Error> {
    let world = CodeSpanReportWorld::new(world);

    let mut w = match diagnostic_format {
        DiagnosticFormat::Human => color_stream(),
        DiagnosticFormat::Short => StandardStream::stderr(ColorChoice::Never),
    };

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

        term::emit(&mut w, &config, &world, &diag)?;

        // Stacktrace-like helper diagnostics.
        for point in &diagnostic.trace {
            let message = point.v.to_string();
            let help = Diagnostic::help()
                .with_message(message)
                .with_labels(label(world.world, point.span).into_iter().collect());

            term::emit(&mut w, &config, &world, &help)?;
        }
    }

    Ok(())
}

/// Create a label for a span.
fn label(world: &dyn SourceWorld, span: Span) -> Option<Label<FileId>> {
    Some(Label::primary(span.id()?, world.range(span)?))
}
