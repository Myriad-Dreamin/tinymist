use std::sync::OnceLock;

use regex::Regex;

use super::*;

pub fn snapshot_testing(name: &str, f: &impl Fn(LspWorld, PathBuf)) {
    tinymist_tests::snapshot_testing!(name, |verse, path| {
        f(verse.snapshot(), path);
    });
}

#[test]
fn convert() {
    snapshot_testing("integration", &|world, _path| {
        insta::assert_snapshot!(conv(world, false));
    });
}

#[test]
fn convert_docs() {
    snapshot_testing("docs", &|world, _path| {
        insta::assert_snapshot!(conv(world, true));
    });
}

fn conv(world: LspWorld, for_docs: bool) -> EcoString {
    let converter = Typlite::new(Arc::new(world)).with_feature(TypliteFeat {
        annotate_elem: for_docs,
        ..Default::default()
    });
    match converter.convert() {
        Ok(conv) => {
            static REG: OnceLock<Regex> = OnceLock::new();
            let reg =
                REG.get_or_init(|| Regex::new(r#"data:image/svg\+xml;base64,([^"]+)"#).unwrap());
            let res = reg.replace_all(&conv, |_captures: &regex::Captures| {
                "data:image-hash/svg+xml;base64,redacted"
            });

            res.into()
        }
        Err(err) => format!("failed to convert to markdown: {err}").into(),
    }
}
