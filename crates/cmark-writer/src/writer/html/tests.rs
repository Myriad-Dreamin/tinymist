use crate::ast::{HtmlAttribute, HtmlElement, Node};
use crate::{HtmlWriter, HtmlWriterOptions};

#[test]
fn write_trusted_html_keeps_fragment_verbatim() {
    let mut writer = HtmlWriter::new();
    writer.start_tag("div").unwrap();
    writer.finish_tag().unwrap();
    writer.write_trusted_html("<span>").unwrap();
    writer.write_trusted_html("&ok").unwrap();
    writer.end_tag("div").unwrap();

    let output = writer.into_string().unwrap();
    assert_eq!(output, "<div><span>&ok</div>");
}

#[test]
fn write_untrusted_html_escapes_fragment() {
    let mut writer = HtmlWriter::new();
    writer.start_tag("div").unwrap();
    writer.finish_tag().unwrap();
    writer.write_untrusted_html("<span>&oops").unwrap();
    writer.end_tag("div").unwrap();

    let output = writer.into_string().unwrap();
    assert_eq!(output, "<div>&lt;span&gt;&amp;oops</div>");
}

#[test]
fn attribute_escaping_handles_quotes_and_special_chars() {
    let mut writer = HtmlWriter::new();
    writer.start_tag("div").unwrap();
    writer
        .attribute("data-title", "He said \"<Hello>\" & more")
        .unwrap();
    writer.finish_self_closing_tag().unwrap();

    let output = writer.into_string().unwrap();
    assert_eq!(
        output,
        "<div data-title=\"He said &quot;&lt;Hello&gt;&quot; &amp; more\" />"
    );
}

#[test]
fn guarded_writer_renders_safe_html_element() {
    let mut writer = HtmlWriter::new();
    let element = HtmlElement {
        tag: "div".into(),
        attributes: vec![HtmlAttribute {
            name: "class".into(),
            value: "note".into(),
        }],
        children: vec![Node::Text("Hello".into())],
        self_closing: false,
    };

    writer.write_html_element(&element).unwrap();

    let output = writer.into_string().unwrap();
    assert_eq!(output, "<div class=\"note\">\nHello\n</div>");
}

#[test]
fn guarded_writer_textualizes_invalid_tag_in_non_strict_mode() {
    let mut writer = HtmlWriter::with_options(HtmlWriterOptions {
        strict: false,
        ..Default::default()
    });
    let element = HtmlElement {
        tag: "div!".into(),
        attributes: vec![HtmlAttribute {
            name: "class".into(),
            value: "unsafe".into(),
        }],
        children: vec![Node::Text("oops".into())],
        self_closing: false,
    };

    writer.write_html_element(&element).unwrap();

    let output = writer.into_string().unwrap();
    assert_eq!(
        output,
        "&lt;div! class=\"unsafe\"&gt;\noops\n&lt;/div!&gt;"
    );
}

#[test]
fn guarded_writer_errors_on_invalid_tag_in_strict_mode() {
    let mut writer = HtmlWriter::with_options(HtmlWriterOptions {
        strict: true,
        ..Default::default()
    });
    let element = HtmlElement {
        tag: "div!".into(),
        attributes: vec![],
        children: vec![],
        self_closing: true,
    };

    let err = writer.write_html_element(&element).unwrap_err();
    assert!(matches!(err, crate::HtmlWriteError::InvalidHtmlTag(tag) if tag == "div!"));
}

#[test]
fn guarded_writer_textualizes_invalid_attribute_in_non_strict_mode() {
    let mut writer = HtmlWriter::with_options(HtmlWriterOptions {
        strict: false,
        ..Default::default()
    });
    let element = HtmlElement {
        tag: "div".into(),
        attributes: vec![HtmlAttribute {
            name: "onload!".into(),
            value: "evil".into(),
        }],
        children: vec![Node::Text("body".into())],
        self_closing: false,
    };

    writer.write_html_element(&element).unwrap();
    let output = writer.into_string().unwrap();
    assert_eq!(
        output,
        "&lt;div onload!=\"evil\"&gt;\nbody\n&lt;/div&gt;"
    );
}

#[test]
fn guarded_writer_errors_on_invalid_attribute_in_strict_mode() {
    let mut writer = HtmlWriter::with_options(HtmlWriterOptions {
        strict: true,
        ..Default::default()
    });
    let element = HtmlElement {
        tag: "div".into(),
        attributes: vec![HtmlAttribute {
            name: "onload!".into(),
            value: "evil".into(),
        }],
        children: vec![],
        self_closing: true,
    };

    let err = writer.write_html_element(&element).unwrap_err();
    assert!(matches!(err, crate::HtmlWriteError::InvalidHtmlAttribute(name) if name == "onload!"));
}
