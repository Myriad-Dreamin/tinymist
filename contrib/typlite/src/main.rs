#![doc = include_str!("../README.md")]

extern crate typlite;

use std::path::{Path, PathBuf};

use typlite::Typlite;

fn main() {
    let input = std::env::args().nth(1).unwrap();
    let input = Path::new(&input);
    let output = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .or_else(|| Some(input.with_extension("md")))
        .unwrap();

    let input = std::fs::read_to_string(input).unwrap();
    let typlite = Typlite::new_with_content(&input);
    std::fs::write(output, typlite.convert().unwrap().as_str()).unwrap();
}
