//! Generates project metadata.

use anyhow::Result;
use vergen::EmitBuilder;

fn main() -> Result<()> {
    // Check if we're in a git worktree to avoid warnings in containerized builds
    let is_git_available = std::process::Command::new("git")
        .args(&["rev-parse", "--git-dir"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    // Emit the instructions
    let mut builder = EmitBuilder::builder();
    
    if is_git_available {
        // Include git information when available
        builder
            .all_cargo()
            .build_timestamp()
            .git_sha(false)
            .git_describe(true, true, None)
            .all_rustc()
            .emit()?;
    } else {
        // Skip git information to avoid warnings in containerized environments
        println!("cargo:rustc-env=VERGEN_GIT_SHA=unknown");
        println!("cargo:rustc-env=VERGEN_GIT_DESCRIBE=unknown");
        builder
            .all_cargo()
            .build_timestamp()
            .all_rustc()
            .emit()?;
    }

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
