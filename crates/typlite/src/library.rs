//! # Typlite Library

use super::*;
use ecow::eco_format;
use typst_syntax::{ast, SyntaxKind, SyntaxNode};
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
    scopes.define("table", table as RawFunc);
    scopes.define("grid", grid as RawFunc);
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
        (Value::Image { path, alt }, Some(caption)) if args.vm.feat.gfm => Ok(Value::Content(
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

/// Evaluate a table.
pub fn table(mut args: Args) -> Result<Value> {
    let columns = match args.get_named_("columns").unwrap().kind() {
        SyntaxKind::Array => {
            let array: ast::Array = args.get_named_("columns").unwrap().cast().unwrap();
            array.items().count()
        }
        SyntaxKind::Int => {
            let int_val: ast::Int = args.get_named_("columns").unwrap().cast().unwrap();
            int_val.get().try_into().unwrap()
        }
        other => return Err(format!("invalid columns argument of type {:?}", other).into()),
    };

    let header_field = SyntaxNode::inner(
        SyntaxKind::FieldAccess,
        vec![
            SyntaxNode::leaf(SyntaxKind::Ident, "table"),
            SyntaxNode::leaf(SyntaxKind::Dot, "."),
            SyntaxNode::leaf(SyntaxKind::Ident, "header"),
        ],
    );
    let footer_field = SyntaxNode::inner(
        SyntaxKind::FieldAccess,
        vec![
            SyntaxNode::leaf(SyntaxKind::Ident, "table"),
            SyntaxNode::leaf(SyntaxKind::Dot, "."),
            SyntaxNode::leaf(SyntaxKind::Ident, "footer"),
        ],
    );

    let mut header: Vec<EcoString> = Vec::new();
    let mut cells: Vec<EcoString> = Vec::new();

    while let Some(pos_arg) = args.pos.pop() {
        if pos_arg.kind() != SyntaxKind::FuncCall {
            let evaluated = args.vm.eval(pos_arg)?;
            cells.push(TypliteWorker::value(evaluated));
        } else {
            let func_call: ast::FuncCall = pos_arg.cast().unwrap();
            let first_child = pos_arg.children().next().unwrap();

            if header_field.spanless_eq(first_child) {
                let mut header_args = Args::new(args.vm, func_call.args());
                while let Some(arg) = header_args.pos.pop() {
                    let evaluated = header_args.vm.eval(arg)?;
                    header.push(TypliteWorker::value(evaluated));
                }
            } else {
                let evaluated = args.vm.eval(pos_arg)?;
                cells.push(TypliteWorker::value(evaluated));
            }
            if footer_field.spanless_eq(first_child) {
                let mut footer_args = Args::new(args.vm, func_call.args());
                while let Some(arg) = footer_args.pos.pop() {
                    let evaluated = footer_args.vm.eval(arg)?;
                    cells.push(TypliteWorker::value(evaluated));
                }
            }
        }
    }

    if header.is_empty() {
        if cells.len() < columns {
            return Err("not enough cells to form header".into());
        }
        header = cells.drain(0..columns).collect();
    }

    let mut res = EcoString::new();
    res.push('|');
    for cell in &header {
        res.push_str(&format!(" {} |", cell));
    }
    res.push('\n');

    res.push('|');
    for _ in 0..columns {
        res.push_str(" --- |");
    }
    res.push('\n');

    for row in cells.chunks(columns) {
        res.push('|');
        for cell in row {
            res.push_str(&format!(" {} |", cell));
        }
        res.push('\n');
    }

    Ok(Value::Content(res))
}

/// Evaluate a grid.
pub fn grid(mut args: Args) -> Result<Value> {
    let columns = match args.get_named_("columns").unwrap().kind() {
        SyntaxKind::Array => {
            let array: ast::Array = args.get_named_("columns").unwrap().cast().unwrap();
            array.items().count()
        }
        SyntaxKind::Int => {
            let int_val: ast::Int = args.get_named_("columns").unwrap().cast().unwrap();
            int_val.get().try_into().unwrap()
        }
        other => return Err(format!("invalid columns argument of type {:?}", other).into()),
    };

    let header_field = SyntaxNode::inner(
        SyntaxKind::FieldAccess,
        vec![
            SyntaxNode::leaf(SyntaxKind::Ident, "grid"),
            SyntaxNode::leaf(SyntaxKind::Dot, "."),
            SyntaxNode::leaf(SyntaxKind::Ident, "header"),
        ],
    );
    let footer_field = SyntaxNode::inner(
        SyntaxKind::FieldAccess,
        vec![
            SyntaxNode::leaf(SyntaxKind::Ident, "grid"),
            SyntaxNode::leaf(SyntaxKind::Dot, "."),
            SyntaxNode::leaf(SyntaxKind::Ident, "footer"),
        ],
    );

    let mut header: Vec<EcoString> = Vec::new();
    let mut cells: Vec<EcoString> = Vec::new();

    while let Some(pos_arg) = args.pos.pop() {
        if pos_arg.kind() != SyntaxKind::FuncCall {
            let evaluated = args.vm.eval(pos_arg)?;
            cells.push(TypliteWorker::value(evaluated));
        } else {
            let func_call: ast::FuncCall = pos_arg.cast().unwrap();
            let first_child = pos_arg.children().next().unwrap();

            if header_field.spanless_eq(first_child) {
                let mut header_args = Args::new(args.vm, func_call.args());
                while let Some(arg) = header_args.pos.pop() {
                    let evaluated = header_args.vm.eval(arg)?;
                    header.push(TypliteWorker::value(evaluated));
                }
            } else {
                let evaluated = args.vm.eval(pos_arg)?;
                cells.push(TypliteWorker::value(evaluated));
            }
            if footer_field.spanless_eq(first_child) {
                let mut footer_args = Args::new(args.vm, func_call.args());
                while let Some(arg) = footer_args.pos.pop() {
                    let evaluated = footer_args.vm.eval(arg)?;
                    cells.push(TypliteWorker::value(evaluated));
                }
            }
        }
    }

    if header.is_empty() {
        if cells.len() < columns {
            return Err("not enough cells to form header".into());
        }
        header = cells.drain(0..columns).collect();
    }

    let mut res = EcoString::new();
    res.push('|');
    for cell in &header {
        res.push_str(&format!(" {} |", cell));
    }
    res.push('\n');

    res.push('|');
    for _ in 0..columns {
        res.push_str(" --- |");
    }
    res.push('\n');

    for row in cells.chunks(columns) {
        res.push('|');
        for cell in row {
            res.push_str(&format!(" {} |", cell));
        }
        res.push('\n');
    }

    Ok(Value::Content(res))
}
