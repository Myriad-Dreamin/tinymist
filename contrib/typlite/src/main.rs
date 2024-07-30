#![doc = include_str!("../README.md")]

extern crate typlite;

use std::path::{Path, PathBuf};

use typlite::Typlite;

fn main() {
    let input = std::env::args().nth(1).unwrap();
    let input = Path::new(&input);
    let output = match std::env::args().nth(2) {
        Some(e) if e == "-" => None,
        Some(e) => Some(PathBuf::from(e)),
        None => Some(input.with_extension("md")),
    };

    let input = std::fs::read_to_string(input).unwrap();
    let conv = Typlite::new_with_content(&input).convert();

    match (conv, output) {
        (Ok(conv), None) => println!("{}", conv),
        (Ok(conv), Some(output)) => std::fs::write(output, conv.as_str()).unwrap(),
        (Err(e), ..) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
