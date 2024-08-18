#![doc = include_str!("../README.md")]

extern crate clap;
extern crate tinymist_world;
extern crate typlite;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::Parser;
use tinymist_world::{EntryState, LspUniverseBuilder};
use typlite::{CompileOnceArgs, Typlite};

/// Common arguments of compile, watch, and query.
#[derive(Debug, Clone, Parser, Default)]
pub struct CompileArgs {
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Path to ouput Markdown file
    #[clap(value_name = "OUTPUT")]
    pub output: Option<String>,
}

fn main() -> typlite::Result<()> {
    // Parse command line arguments
    let args = CompileArgs::parse();

    let input = args
        .compile
        .input
        .ok_or("Missing required argument: INPUT")?;
    let input = Path::new(&input);
    let output = match args.output {
        Some(e) if e == "-" => None,
        Some(e) => Some(PathBuf::from(e)),
        None => Some(input.with_extension("md")),
    };

    let font_resolver =
        LspUniverseBuilder::resolve_fonts(args.compile.font).map_err(|e| format!("{e:?}"))?;
    // todo: check input to in root
    let root = args
        .compile
        .root
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let universe = LspUniverseBuilder::build(
        EntryState::new_workspace(root.as_path().into()),
        Arc::new(font_resolver),
        Default::default(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let world = universe.snapshot();

    let input = std::fs::read_to_string(input).unwrap();
    let conv = Typlite::new_with_content(&input)
        .with_world(world)
        .convert();

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
