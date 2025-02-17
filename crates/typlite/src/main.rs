#![doc = include_str!("../README.md")]

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::Parser;
use ecow::{eco_format, EcoString};
use tinymist_project::WorldProvider;
use typlite::{value::*, TypliteFeat};
use typlite::{CompileOnceArgs, Typlite};

/// Common arguments of compile, watch, and query.
#[derive(Debug, Clone, Parser, Default)]
pub struct CompileArgs {
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Path to output file
    #[clap(value_name = "OUTPUT")]
    pub output: Option<String>,

    /// Configures the path of assets directory
    #[clap(long, default_value = None, value_name = "ASSETS_PATH")]
    pub assets_path: Option<String>,

    /// Configure the path to the assets' corresponding source code directory. When the path is specified, typlite adds a href to jump to the source code in the exported asset.
    #[clap(long, default_value = None, value_name = "ASSETS_SRC_PATH")]
    pub assets_src_path: Option<String>,
}

fn main() -> typlite::Result<()> {
    // Parse command line arguments
    let args = CompileArgs::parse();

    let input = args
        .compile
        .input
        .as_ref()
        .ok_or("Missing required argument: INPUT")?;
    let output = match args.output {
        Some(stdout_path) if stdout_path == "-" => None,
        Some(output_path) => Some(PathBuf::from(output_path)),
        None => Some(Path::new(input).with_extension("md")),
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
    let assets_src_path = match args.assets_src_path {
        Some(assets_src_path) => {
            let path = PathBuf::from(assets_src_path);
            if !path.exists() {
                if let Err(e) = std::fs::create_dir_all(&path) {
                    return Err(format!("failed to create assets' src directory: {}", e).into());
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
            assets_path,
            assets_src_path,
            ..Default::default()
        });
    let conv = converter.convert();

    match (conv, output) {
        (Ok(conv), None) => println!("{}", conv),
        (Ok(conv), Some(output)) => std::fs::write(output, conv.as_str()).unwrap(),
        (Err(err), ..) => {
            eprintln!("{err}");
            std::process::exit(1);
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

    Ok(Value::Content(eco_format!(
        "[{body}](https://myriad-dreamin.github.io/tinymist/{dest})",
        dest = dest.to_string_lossy()
    )))
}
