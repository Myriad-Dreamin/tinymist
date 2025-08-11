//! Generates project metadata.

use anyhow::Result;
use vergen::EmitBuilder;

fn main() -> Result<()> {
    // Emit the instructions
    EmitBuilder::builder()
        .all_cargo()
        .build_timestamp()
        .git_sha(false)
        .git_describe(true, true, None)
        .all_rustc()
        .emit()?;

    let metadata = cargo_metadata::MetadataCommand::new().exec().unwrap();
    let typst = metadata
        .packages
        .iter()
        .find(|package| package.name == "typst")
        .expect("Typst should be a dependency");

    println!("cargo:rustc-env=TYPST_VERSION={}", typst.version);
    let src = typst
        .source
        .as_ref()
        .map(|e| e.repr.as_str())
        .unwrap_or_default();
    println!("cargo:rustc-env=TYPST_SOURCE={src}");
    Ok(())
}
