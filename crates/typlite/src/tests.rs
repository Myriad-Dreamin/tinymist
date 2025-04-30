use std::sync::OnceLock;

use regex::Regex;
use typst::html::{HtmlNode, HtmlTag};
use typst_syntax::Span;

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

#[test]
fn test_docx_generation() {
    snapshot_testing("integration", &|world, _path| {
        let converter = Typlite::new(Arc::new(world.clone()))
            .with_feature(TypliteFeat {
                ..Default::default()
            })
            .with_format(Format::Docx);

        let docx_data = match converter.to_docx() {
            Ok(data) => data,
            Err(err) => {
                panic!("Failed to generate DOCX: {}", err);
            }
        };

        assert!(!docx_data.is_empty(), "DOCX data should not be empty");

        assert_eq!(
            &docx_data[0..2],
            &[0x50, 0x4B],
            "DOCX data should start with PK signature"
        );

        // insta::assert_binary_snapshot!("test_output.docx", docx_data);

        let hash = format!("{:x}", md5::compute(&docx_data));
        insta::assert_snapshot!(hash);
    });
}

fn conv(world: LspWorld, for_docs: bool) -> String {
    let converter = Typlite::new(Arc::new(world)).with_feature(TypliteFeat {
        annotate_elem: for_docs,
        ..Default::default()
    });
    let doc = match converter.convert_doc() {
        Ok(doc) => doc,
        Err(err) => return format!("failed to convert to markdown: {err}"),
    };

    let repr = typst_html::html(&redact(doc.base.clone())).unwrap();
    let res = doc.to_md_string().unwrap();
    static REG: OnceLock<Regex> = OnceLock::new();
    let reg = REG.get_or_init(|| Regex::new(r#"data:image/svg\+xml;base64,([^"]+)"#).unwrap());
    let res = reg.replace_all(&res, |_captures: &regex::Captures| {
        "data:image-hash/svg+xml;base64,redacted"
    });

    [repr.as_str(), res.as_ref()].join("\n=====\n")
}

fn redact(doc: HtmlDocument) -> HtmlDocument {
    let mut doc = doc;
    for node in doc.root.children.iter_mut() {
        redact_node(node);
    }
    doc
}

fn redact_node(node: &mut HtmlNode) {
    match node {
        HtmlNode::Element(elem) => {
            if elem.tag == HtmlTag::constant("svg") {
                elem.children = vec![];
            } else {
                for child in elem.children.iter_mut() {
                    redact_node(child);
                }
            }
        }
        HtmlNode::Frame(_) => {
            *node = HtmlNode::Text("redacted-frame".into(), Span::detached());
        }
        _ => {}
    }
}
