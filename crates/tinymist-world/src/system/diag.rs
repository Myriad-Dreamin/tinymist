use std::io::IsTerminal;

use codespan_reporting::term::termcolor::{ColorChoice, StandardStream, WriteColor};

use ecow::EcoString;
use tinymist_std::Result;
use typst::diag::{SourceDiagnostic, StrResult};

use crate::{DiagnosticFormat, SourceWorld};

/// Prints diagnostic messages to the terminal.
#[deprecated(note = "Use `diag` mod instead")]
pub fn print_diagnostics_to<'d, 'files>(
    world: &'files dyn SourceWorld,
    errors: impl Iterator<Item = &'d SourceDiagnostic>,
    w: &mut impl WriteColor,
    diagnostic_format: DiagnosticFormat,
) -> Result<(), codespan_reporting::files::Error> {
    crate::diag::print_diagnostics_to(world, errors, w, diagnostic_format)
}

/// Prints diagnostic messages to the terminal.
#[deprecated(note = "Use `diag` mod instead")]
pub fn print_diagnostics_to_string<'d, 'files>(
    world: &'files dyn SourceWorld,
    errors: impl Iterator<Item = &'d SourceDiagnostic>,
    diagnostic_format: DiagnosticFormat,
) -> StrResult<EcoString> {
    crate::diag::print_diagnostics_to_string(world, errors, diagnostic_format)
}

/// Gets stderr with color support if desirable.
fn color_stream() -> StandardStream {
    StandardStream::stderr(if std::io::stderr().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    })
}

/// Prints diagnostic messages to the terminal.
pub fn print_diagnostics<'d, 'files>(
    world: &'files dyn SourceWorld,
    errors: impl Iterator<Item = &'d SourceDiagnostic>,
    diagnostic_format: DiagnosticFormat,
) -> Result<(), codespan_reporting::files::Error> {
    let mut w = match diagnostic_format {
        DiagnosticFormat::Human => color_stream(),
        DiagnosticFormat::Short => StandardStream::stderr(ColorChoice::Never),
    };

    crate::diag::print_diagnostics_to(world, errors, &mut w, diagnostic_format)
}
