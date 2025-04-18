//! Tests support for tinymist crates.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use tinymist_project::{
    base::ShadowApi, CompileFontArgs, EntryManager, EntryState, ExportTarget, LspUniverse,
    LspUniverseBuilder,
};
use typst::{foundations::Bytes, syntax::VirtualPath};

/// A test that runs a function with a given source string and returns the
/// result.
///
/// Multiple sources can be provided, separated by `-----`. The last source
/// is used as the entry point.
pub fn run_with_sources<T>(source: &str, f: impl FnOnce(&mut LspUniverse, PathBuf) -> T) -> T {
    let root = if cfg!(windows) {
        PathBuf::from("C:\\root")
    } else {
        PathBuf::from("/root")
    };
    let mut verse = LspUniverseBuilder::build(
        EntryState::new_rooted(root.as_path().into(), None),
        ExportTarget::Paged,
        Default::default(),
        Default::default(),
        LspUniverseBuilder::resolve_package(None, None),
        Arc::new(
            LspUniverseBuilder::resolve_fonts(CompileFontArgs {
                ignore_system_fonts: true,
                ..Default::default()
            })
            .unwrap(),
        ),
    );
    let sources = source.split("-----");

    let mut last_pw = None;
    for (idx, source) in sources.enumerate() {
        // find prelude
        let mut source = source.trim_start();
        let mut path = None;

        if source.starts_with("//") {
            let first_line = source.lines().next().unwrap();
            let content = first_line.trim_start_matches("/").trim();

            if let Some(path_attr) = content.strip_prefix("path:") {
                source = source.strip_prefix(first_line).unwrap().trim();
                path = Some(path_attr.trim().to_owned())
            }
        };

        let path = path.unwrap_or_else(|| format!("/s{idx}.typ"));
        let path = path.strip_prefix("/").unwrap_or(path.as_str());

        let pw = root.join(Path::new(&path));
        verse
            .map_shadow(&pw, Bytes::from_string(source.to_owned()))
            .unwrap();
        last_pw = Some(pw);
    }

    let pw = last_pw.unwrap();
    verse
        .mutate_entry(EntryState::new_rooted(
            root.as_path().into(),
            Some(VirtualPath::new(pw.strip_prefix(root).unwrap())),
        ))
        .unwrap();
    f(&mut verse, pw)
}
