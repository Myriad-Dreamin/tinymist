#![doc = include_str!("../README.md")]

extern crate clap;
extern crate tinymist_world;
extern crate typlite;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::Parser;
use typlite::{CompileOnceArgs, Typlite};

/// Common arguments of compile, watch, and query.
#[derive(Debug, Clone, Parser, Default)]
pub struct CompileArgs {
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Path to ouput file
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

    let conv = Typlite::new(Arc::new(world)).convert();

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
