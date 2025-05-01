#![doc = include_str!("../README.md")]

use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::Parser;
use ecow::{eco_format, EcoString};
use tinymist_project::WorldProvider;
use typlite::{common::Format, value::*, TypliteFeat};
use typlite::{CompileOnceArgs, Typlite};

/// Common arguments of compile, watch, and query.
#[derive(Debug, Clone, Parser, Default)]
pub struct CompileArgs {
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Path to output file(s)
    #[clap(value_name = "OUTPUT", action = clap::ArgAction::Append)]
    pub outputs: Vec<String>,

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

    let outputs = if args.outputs.is_empty() {
        vec![Path::new(input)
            .with_extension("md")
            .to_string_lossy()
            .to_string()]
    } else {
        args.outputs.clone()
    };

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

    let converter = Typlite::new(Arc::new(world))
        .with_library(lib())
        .with_feature(TypliteFeat {
            assets_path: assets_path.clone(),
            ..Default::default()
        });
    let doc = match converter.convert_doc() {
        Ok(doc) => doc,
        Err(err) => return Err(format!("failed to convert document: {err}").into()),
    };

    for output_path in &outputs {
        let is_stdout = output_path == "-";
        let output = if is_stdout {
            None
        } else {
            Some(PathBuf::from(output_path))
        };

        let format = match &output {
            Some(output) if output.extension() == Some(std::ffi::OsStr::new("tex")) => {
                Format::LaTeX
            }
            Some(output) if output.extension() == Some(std::ffi::OsStr::new("docx")) => {
                Format::Docx
            }
            _ => Format::Md,
        };

        match format {
            Format::Docx => todo!(),
            Format::LaTeX => {
                let result = doc.to_tex_string(true);
                match (result, output) {
                    (Ok(content), None) => {
                        std::io::stdout()
                            .write_all(content.as_str().as_bytes())
                            .unwrap();
                    }
                    (Ok(content), Some(output)) => {
                        if let Err(err) = std::fs::write(&output, content.as_str()) {
                            eprintln!("failed to write LaTeX file {}: {}", output.display(), err);
                            continue;
                        }
                        println!("Generated LaTeX file: {}", output.display());
                    }
                    (Err(err), _) => {
                        eprintln!("Error converting to LaTeX for {}: {}", output_path, err);
                        continue;
                    }
                }
            }
            Format::Md => {
                let result = doc.to_md_string();
                match (result, output) {
                    (Ok(content), None) => {
                        std::io::stdout()
                            .write_all(content.as_str().as_bytes())
                            .unwrap();
                    }
                    (Ok(content), Some(output)) => {
                        if let Err(err) = std::fs::write(&output, content.as_str()) {
                            eprintln!(
                                "failed to write Markdown file {}: {}",
                                output.display(),
                                err
                            );
                            continue;
                        }
                        println!("Generated Markdown file: {}", output.display());
                    }
                    (Err(err), _) => {
                        eprintln!("Error converting to Markdown for {}: {}", output_path, err);
                        continue;
                    }
                }
            }
        }
    }

    Ok(())
}

fn lib() -> Arc<typlite::scopes::Scopes<Value>> {
    let mut scopes = typlite::library::docstring_lib();

    // todo: how to import this function correctly?
    scopes.define("cross-link", cross_link as RawFunc);

    Arc::new(scopes)
}

/// Evaluate a `cross-link`.
pub fn cross_link(mut args: Args) -> typlite::Result<Value> {
    let dest = get_pos_named!(args, dest: EcoString);
    let body = get_pos_named!(args, body: Content);

    let dest = std::path::Path::new(dest.as_str()).with_extension("html");
    let mut dest = dest.as_path();

    // strip leading `/` from the path
    if let Ok(s) = dest.strip_prefix("/") {
        dest = s;
    }

    Ok(Value::Content(eco_format!(
        "[{body}](https://myriad-dreamin.github.io/tinymist/{dest})",
        dest = dest.to_string_lossy()
    )))
}
