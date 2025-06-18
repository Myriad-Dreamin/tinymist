#![doc = include_str!("../README.md")]

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::exit,
    sync::Arc,
};

use clap::Parser;
use tinymist_project::{
    base::print_diagnostics, DiagnosticFormat, LspWorld, SourceWorld, WorldProvider,
};
use tinymist_std::{error::prelude::*, Result};
use typlite::{common::Format, TypliteFeat};
use typlite::{CompileOnceArgs, Typlite};
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
    /// compilation.
    ///
    /// typlite exactly uses the `#article` function to process the content as
    /// follow:
    ///
    /// ```typst
    /// #import "@local/processor": article
    /// #article(include "the-processed-content.typ")
    /// ```
    ///
    /// It resembles the regular typst show rule function, like `#show:
    /// article`.
    #[clap(long = "processor", default_value = None, value_name = "PACKAGE_SPEC")]
    pub processor: Option<String>,
}

fn main() -> Result<()> {
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

    if let Some(assets_path) = args.assets_path.as_ref() {
        if !assets_path.exists() {
            std::fs::create_dir_all(assets_path).context("failed to create assets directory")?;
        }
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

    if is_stdout {
        std::io::stdout()
            .write_all(result.as_slice())
            .context("failed to write to stdout")?;
    } else if let Err(err) = std::fs::write(&output_path, result.as_slice()) {
        bail!("failed to write file {output_path:?}: {err}");
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
