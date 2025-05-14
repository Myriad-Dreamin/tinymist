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
use tinymist_std::error::prelude::*;
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
    pub assets_path: Option<String>,
}

fn main() -> tinymist_std::Result<()> {
    // Parse command line arguments
    let args = CompileArgs::parse();

    let universe = args.compile.resolve()?;
    let world = Arc::new(universe.snapshot());

    print_diag_or_error(world.as_ref(), run(args, world.clone()))
}

fn run(args: CompileArgs, world: Arc<LspWorld>) -> tinymist_std::Result<()> {
    let input = args
        .compile
        .input
        .as_ref()
        .context("Missing required argument: INPUT")?;

    let is_stdout = args.output.as_deref() == Some("-");
    let output_path = args
        .output
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(input).with_extension("md"));

    let output_format = match output_path.extension() {
        Some(ext) if ext == std::ffi::OsStr::new("tex") => Format::LaTeX,
        Some(ext) if ext == std::ffi::OsStr::new("txt") => Format::Text,
        #[cfg(feature = "docx")]
        Some(ext) if ext == std::ffi::OsStr::new("docx") => Format::Docx,
        _ => Format::Md,
    };

    let assets_path = match args.assets_path {
        Some(assets_path) => {
            let path = PathBuf::from(assets_path);
            if !path.exists() {
                if let Err(e) = std::fs::create_dir_all(&path) {
                    bail!("failed to create assets directory: {e}");
                }
            }
            Some(path)
        }
        None => None,
    };

    let converter = Typlite::new(world).with_feature(TypliteFeat {
        assets_path: assets_path.clone(),
        ..Default::default()
    });
    let doc = converter.convert_doc(output_format)?;

    let result = match output_format {
        Format::Md => Bytes::from_string(doc.to_md_string()?),
        Format::LaTeX => Bytes::from_string(doc.to_tex_string(true)?),
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

fn print_diag_or_error<T>(
    world: &impl SourceWorld,
    result: tinymist_std::Result<T>,
) -> tinymist_std::Result<T> {
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
