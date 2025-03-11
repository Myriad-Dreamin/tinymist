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
use typst::diag::{eco_format, Severity, SourceDiagnostic};
use typst::syntax::Span;
use typst::{World, WorldExt};

use crate::{DiagnosticFormat, SourceWorld};

/// Get stderr with color support if desirable.
fn color_stream() -> StandardStream {
    StandardStream::stderr(if std::io::stderr().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    })
}

/// Print diagnostic messages to the terminal.
pub fn print_diagnostics<'d, 'files, W: SourceWorld>(
    world: &'files W,
    errors: impl Iterator<Item = &'d SourceDiagnostic>,
    diagnostic_format: DiagnosticFormat,
) -> Result<(), codespan_reporting::files::Error> {
    let world = world.for_codespan_reporting();

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
fn label<W: World>(world: &W, span: Span) -> Option<Label<FileId>> {
    Some(Label::primary(span.id()?, world.range(span)?))
}
