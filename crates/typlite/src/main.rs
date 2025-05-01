#![doc = include_str!("../README.md")]

use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::Parser;
use tinymist_project::WorldProvider;
use typlite::{common::Format, AssetsHandler, SystemAssetsHandler, TypliteFeat};
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

fn main() -> typlite::Result<()> {
    // Parse command line arguments
    let args = CompileArgs::parse();

    let input = args
        .compile
        .input
        .as_ref()
        .ok_or("Missing required argument: INPUT")?;

    let is_stdout = args.output.as_deref() == Some("-");
    let output_path = args
        .output
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(input).with_extension("md"));

    let output_format = match output_path.extension() {
        Some(ext) if ext == std::ffi::OsStr::new("tex") => Format::LaTeX,
        #[cfg(feature = "docx")]
        Some(ext) if ext == std::ffi::OsStr::new("docx") => Format::Docx,
        _ => Format::Md,
    };

    if matches!(output_format, Format::LaTeX) && args.assets_path.is_none() {
        return Err("LaTeX output requires an assets path".into());
    }

    let assets_path = match args.assets_path {
        Some(assets_path) => {
            let path = PathBuf::from(assets_path);
            if !path.exists() {
                if let Err(e) = std::fs::create_dir_all(&path) {
                    return Err(format!("failed to create assets directory: {}", e).into());
                }
            }
            Some(path)
        }
        None => None,
    };

    let universe = args.compile.resolve().map_err(|err| format!("{err:?}"))?;
    let world = universe.snapshot();

    type AH = Arc<dyn AssetsHandler>;
    let converter = Typlite::new(Arc::new(world)).with_feature(TypliteFeat {
        assets_handler: assets_path
            .map(|assets_path| Arc::new(SystemAssetsHandler::new(assets_path)) as AH),
        ..Default::default()
    });
    let doc = converter.convert_doc(output_format)?;

    let result = match output_format {
        Format::Md => Bytes::from_string(doc.to_md_string()?),
        Format::LaTeX => Bytes::from_string(doc.to_tex_string(true)?),
        #[cfg(feature = "docx")]
        Format::Docx => Bytes::new(doc.to_docx()?),
    };

    if is_stdout {
        std::io::stdout().write_all(result.as_slice()).unwrap();
    } else if let Err(err) = std::fs::write(&output_path, result.as_slice()) {
        Err(format!(
            "failed to write file {}: {err}",
            output_path.display()
        ))?;
    }

    Ok(())
}
