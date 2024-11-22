#![doc = include_str!("../README.md")]

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::Parser;
use ecow::{eco_format, EcoString};
use typlite::value::*;
use typlite::{CompileOnceArgs, Typlite};

/// Common arguments of compile, watch, and query.
#[derive(Debug, Clone, Parser, Default)]
pub struct CompileArgs {
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Path to output file
    #[clap(value_name = "OUTPUT")]
    pub output: Option<String>,
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
        Some(e) if e == "-" => None,
        Some(e) => Some(PathBuf::from(e)),
        None => Some(Path::new(input).with_extension("md")),
    };

    let universe = args.compile.resolve().map_err(|e| format!("{e:?}"))?;
    let world = universe.snapshot();

    let converter = Typlite::new(Arc::new(world)).with_library(lib());
    let conv = converter.convert();

    match (conv, output) {
        (Ok(conv), None) => println!("{}", conv),
        (Ok(conv), Some(output)) => std::fs::write(output, conv.as_str()).unwrap(),
        (Err(e), ..) => {
            eprintln!("{e}");
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
