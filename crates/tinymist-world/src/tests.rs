use clap::Parser;

use crate::args::CompileOnceArgs;

#[test]
#[cfg(feature = "system")]
fn test_args() {
    use tinymist_std::typst::TypstPagedDocument;

    let args = CompileOnceArgs::parse_from(["tinymist", "main.typ"]);
    let verse = args
        .resolve_system()
        .expect("failed to resolve system universe");

    let world = verse.snapshot();
    let _res = typst::compile::<TypstPagedDocument>(&world);
}
