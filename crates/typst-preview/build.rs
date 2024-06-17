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
    Ok(())
}
