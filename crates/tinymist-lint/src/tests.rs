pub use insta::assert_snapshot;
use typst::syntax::Source;

pub fn snapshot_testing(name: &str, f: &impl Fn(Source)) {
    let mut settings = insta::Settings::new();
    settings.set_prepend_module_to_snapshot(false);
    settings.set_snapshot_path(format!("fixtures/{name}/snaps"));
    settings.bind(|| {
        let glob_path = format!("fixtures/{name}/*.typ");
        insta::glob!(&glob_path, |path| {
            let contents = std::fs::read_to_string(path).unwrap();
            #[cfg(windows)]
            let contents = contents.replace("\r\n", "\n");

            f(Source::detached(contents));
        });
    });
}
