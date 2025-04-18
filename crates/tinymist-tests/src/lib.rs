//! Tests support for tinymist crates.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

use tinymist_project::{
    base::ShadowApi, font::FontResolverImpl, CompileFontArgs, EntryManager, EntryState,
    ExportTarget, LspUniverse, LspUniverseBuilder,
};
use typst::{foundations::Bytes, syntax::VirtualPath};

pub use insta::{assert_debug_snapshot, assert_snapshot, glob, with_settings, Settings};

/// Runs snapshot tests.
#[macro_export]
macro_rules! snapshot_testing {
    ($name:expr, $f:expr) => {
        let name = $name;
        let name = if name.is_empty() { "playground" } else { name };
        let mut settings = $crate::Settings::new();
        settings.set_prepend_module_to_snapshot(false);
        settings.set_snapshot_path(format!("fixtures/{name}/snaps"));
        settings.bind(|| {
            let glob_path = format!("fixtures/{name}/*.typ");
            $crate::glob!(&glob_path, |path| {
                let contents = std::fs::read_to_string(path).unwrap();
                #[cfg(windows)]
                let contents = contents.replace("\r\n", "\n");

                $crate::run_with_sources(&contents, $f);
            });
        });
    };
}

/// A test that runs a function with a given source string and returns the
/// result.
///
/// Multiple sources can be provided, separated by `-----`. The last source
/// is used as the entry point.
pub fn run_with_sources<T>(source: &str, f: impl FnOnce(&mut LspUniverse, PathBuf) -> T) -> T {
    static FONT_RESOLVER: LazyLock<Arc<FontResolverImpl>> = LazyLock::new(|| {
        Arc::new(
            LspUniverseBuilder::resolve_fonts(CompileFontArgs {
                ignore_system_fonts: true,
                ..Default::default()
            })
            .unwrap(),
        )
    });

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
        FONT_RESOLVER.clone(),
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
