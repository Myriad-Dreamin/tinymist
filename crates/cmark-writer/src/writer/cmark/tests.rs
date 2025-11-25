use super::CommonMarkWriter;
use crate::ast::{HeadingType, Node};
use crate::options::WriterOptions;
use crate::writer::runtime::diagnostics::{DiagnosticSeverity, SharedVecSink};
use std::cell::RefCell;
use std::rc::Rc;

fn render(node: &Node) -> String {
    let mut writer = CommonMarkWriter::new();
    writer.write(node).unwrap();
    writer.into_string().into()
}

#[test]
fn paragraphs_are_separated_by_blank_line() {
    let document = Node::Document(vec![
        Node::Paragraph(vec![Node::Text("First".into())]),
        Node::Paragraph(vec![Node::Text("Second".into())]),
    ]);

    assert_eq!(render(&document), "First\n\nSecond\n");
}

#[test]
fn setext_heading_matches_content_width() {
    let heading = Node::Heading {
        level: 2,
        content: vec![Node::Text("Wide Title".into())],
        heading_type: HeadingType::Setext,
    };

    assert_eq!(render(&heading), "Wide Title\n----------\n");
}

#[test]
fn autolink_preserves_url() {
    let autolink = Node::Autolink {
        url: "example.com/path".into(),
        is_email: false,
    };

    assert_eq!(render(&autolink), "<example.com/path>");
}

#[test]
fn thematic_break_ends_with_newline() {
    let document = Node::Document(vec![
        Node::ThematicBreak,
        Node::Paragraph(vec![Node::Text("After".into())]),
    ]);

    assert_eq!(render(&document), "---\n\nAfter\n");
}

#[test]
fn diagnostics_capture_warnings() {
    let options = WriterOptions {
        strict: false,
        ..WriterOptions::default()
    };

    let mut writer = CommonMarkWriter::with_options(options);
    let storage = Rc::new(RefCell::new(Vec::new()));
    let sink = SharedVecSink::new(storage.clone());
    writer.set_diagnostic_sink(Box::new(sink));

    let autolink = Node::Autolink {
        url: "foo\nbar".into(),
        is_email: false,
    };

    writer.write(&autolink).unwrap();

    let diagnostics = storage.borrow();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Warning);
    assert!(diagnostics[0]
        .message
        .contains("Newline character found in autolink URL"));
}
