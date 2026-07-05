#![doc = include_str!("../README.md")]
// todo: remove me
#![allow(missing_docs)]

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::exit,
    sync::Arc,
};

use clap::Parser;
use tinymist_project::{
    DiagnosticFormat, LspWorld, SourceWorld, WorldProvider, base::print_diagnostics,
};
use tinymist_std::{Result, error::prelude::*};
use typlite::{CompileOnceArgs, Typlite};
use typlite::{TypliteFeat, common::Format};
use typst::foundations::Bytes;

/// Common arguments of compile, watch, and query.
#[derive(Debug, Clone, Parser, Default)]
pub struct CompileArgs {
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Path to output file
    #[clap(value_name = "OUTPUT", default_value = None)]
    pub output: Option<String>,

    /// Configures the path of assets directory
    #[clap(long, default_value = None, value_name = "ASSETS_PATH")]
    pub assets_path: Option<PathBuf>,

    /// Specifies the package to process markup.
    ///
    /// ## `article` function
    ///
    /// The article function is used to wrap the typst content during
    /// compilation. It resembles the regular typst show rule function, like
    /// `#show: article`.
    ///
    /// typlite exactly uses the `#article` function to process the content as
    /// follow:
    ///
    /// ```typst
    /// #import "@local/ieee-tex:0.1.0": article
    /// #article(include "the-processed-content.typ")
    /// ```
    #[clap(long = "processor", default_value = None, value_name = "PACKAGE_SPEC")]
    pub processor: Option<String>,
}

fn main() -> Result<()> {
    let _ = env_logger::try_init();
    // Parse command line arguments
    let args = CompileArgs::parse();

    let verse = args.compile.resolve()?;
    let world = Arc::new(verse.snapshot());

    print_diag_or_error(world.as_ref(), run(args, world.clone()))
}

fn run(args: CompileArgs, world: Arc<LspWorld>) -> Result<()> {
    let input = args
        .compile
        .input
        .context("Missing required argument: INPUT")?;

    let is_stdout = args.output.as_deref() == Some("-");
    let output_path = args
        .output
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(&input).with_extension("md"));

    let output_format = match output_path.extension().and_then(std::ffi::OsStr::to_str) {
        Some("tex") => Format::LaTeX,
        Some("txt") => Format::Text,
        #[cfg(feature = "docx")]
        Some("docx") => Format::Docx,
        _ => Format::Md,
    };

    if let Some(assets_path) = args.assets_path.as_ref()
        && !assets_path.exists()
    {
        std::fs::create_dir_all(assets_path).context("failed to create assets directory")?;
    }

    let doc = Typlite::new(world.clone())
        .with_feature(TypliteFeat {
            assets_path: args.assets_path,
            processor: args.processor,
            ..Default::default()
        })
        .convert_doc(output_format)?;

    let result = match output_format {
        Format::Md => Bytes::from_string(doc.to_md_string()?),
        Format::LaTeX => Bytes::from_string(doc.to_tex_string()?),
        Format::Text => Bytes::from_string(doc.to_text_string()?),
        #[cfg(feature = "docx")]
        Format::Docx => Bytes::new(doc.to_docx()?),
    };

    let warnings = doc.warnings();

    if is_stdout {
        std::io::stdout()
            .write_all(result.as_slice())
            .context("failed to write to stdout")?;
    } else if let Err(err) = std::fs::write(&output_path, result.as_slice()) {
        bail!("failed to write file {output_path:?}: {err}");
    }

    if !warnings.is_empty() {
        print_diagnostics(world.as_ref(), warnings.iter(), DiagnosticFormat::Human)
            .context_ut("print warnings")?;
    }

    Ok(())
}

fn print_diag_or_error<T>(world: &impl SourceWorld, result: Result<T>) -> Result<T> {
    match result {
        Ok(v) => Ok(v),
        Err(err) => {
            if let Some(diagnostics) = err.diagnostics() {
                print_diagnostics(world, diagnostics.iter(), DiagnosticFormat::Human)
                    .context_ut("print diagnostics")?;
                exit(1);
            }

            Err(err)
        }
    }
}
