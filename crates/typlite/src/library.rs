//! # Typlite Library

use super::*;
use ecow::eco_format;
use value::*;

mod docstring;
pub use docstring::docstring_lib;

pub fn library() -> Scopes<Value> {
    let mut scopes = Scopes::new();
    scopes.define("link", link as RawFunc);
    scopes.define("kbd", kbd as RawFunc);
    scopes.define("md-alter", md_alter as RawFunc);
    scopes.define("image", image as RawFunc);
    scopes.define("figure", figure as RawFunc);
    scopes.define("raw", raw as RawFunc);
    scopes.define("pad", pad as RawFunc);
    scopes.define("note-box", note as RawFunc);
    scopes.define("tip-box", tip as RawFunc);
    scopes.define("important-box", important_box as RawFunc);
    scopes.define("warning-box", warning_box as RawFunc);
    scopes.define("caution-box", caution_box as RawFunc);
    scopes
}

/// Evaluate a link.
pub fn link(mut args: Args) -> Result<Value> {
    let dest = get_pos_named!(args, dest: EcoString);
    let body = get_pos_named!(args, body: Content);

    Ok(Value::Content(eco_format!("[{body}]({dest})")))
}

/// Evaluate an image.
pub fn image(mut args: Args) -> Result<Value> {
    let path = get_pos_named!(args, path: EcoString);
    let alt = get_named!(args, alt: EcoString := "");

    Ok(Value::Image { path, alt })
}

/// Evaluate a figure.
pub fn figure(mut args: Args) -> Result<Value> {
    let body = get_pos_named!(args, path: Value);
    let caption = get_named!(args, caption: Option<Value>).map(TypliteWorker::value);

    match (body, caption) {
        (Value::Image { path, alt }, None) => Ok(Value::Content(eco_format!("![{alt}]({path})"))),
        (Value::Image { path, alt }, Some(caption)) if args.vm.gfm => Ok(Value::Content(
            eco_format!("![{caption}, {alt}]({path} {caption:?})"),
        )),
        (Value::Image { path, alt }, Some(caption)) => {
            Ok(Value::Content(eco_format!("![{caption}, {alt}]({path})")))
        }
        _ => Err("figure only accepts image as body".into()),
    }
}

/// Evaluate a raw.
pub fn raw(mut args: Args) -> Result<Value> {
    let content = get_pos_named!(args, content: EcoString);

    Ok(Value::Content(eco_format!("```` {content} ````")))
}

/// Evaluate a padded content.
pub fn pad(mut args: Args) -> Result<Value> {
    Ok(get_pos_named!(args, path: Value))
}

/// Evaluate a `kbd` element.
pub fn kbd(mut args: Args) -> Result<Value> {
    let key = get_pos_named!(args, key: EcoString);

    Ok(Value::Content(eco_format!("<kbd>{key}</kbd>")))
}

/// Evaluate a markdown alteration.
pub fn md_alter(mut args: Args) -> Result<Value> {
    let _: () = get_pos_named!(args, left: ());
    let right = get_pos_named!(args, right: LazyContent);

    Ok(Value::Content(right.0))
}

/// Evaluate a note.
pub fn note(mut args: Args) -> Result<Value> {
    let body = get_pos_named!(args, body: Content);

    Ok(note_box("NOTE", body))
}

/// Evaluate a tip note box.
pub fn tip(mut args: Args) -> Result<Value> {
    let body = get_pos_named!(args, body: Content);

    Ok(note_box("TIP", body))
}

/// Create a important note box.
pub fn important_box(mut args: Args) -> Result<Value> {
    let body = get_pos_named!(args, body: Content);

    Ok(note_box("IMPORTANT", body))
}

/// Create a warning note box.
pub fn warning_box(mut args: Args) -> Result<Value> {
    let body = get_pos_named!(args, body: Content);

    Ok(note_box("WARNING", body))
}

/// Create a caution note box.
pub fn caution_box(mut args: Args) -> Result<Value> {
    let body = get_pos_named!(args, body: Content);

    Ok(note_box("CAUTION", body))
}

fn note_box(title: &str, body: Content) -> Value {
    let mut res = EcoString::new();
    res.push_str("> [!");
    res.push_str(title);
    res.push_str("]\n");
    let body = body.0;
    for line in body.lines() {
        res.push_str("> ");
        res.push_str(line);
        res.push('\n');
    }

    Value::Content(res)
}
