//! # Typlite Library

use crate::{scopes::Scopes, tinymist_std::typst::diag::EcoString, worker::TypliteWorker};

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
    scopes.define("strike", strike as RawFunc);
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

/// Evaluates a link.
pub fn link(mut args: Args) -> Result<Value> {
    let dest = get_pos_named!(args, dest: EcoString);
    let body = get_pos_named!(args, body: Content);

    Ok(Value::Content(eco_format!("[{body}]({dest})")))
}

/// Evaluates an image.
pub fn image(mut args: Args) -> Result<Value> {
    let path = get_pos_named!(args, path: EcoString);
    let alt = get_named!(args, alt: EcoString := "");

    Ok(Value::Image { path, alt })
}

/// Evaluates a figure.
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

/// Evaluates a raw.
pub fn raw(mut args: Args) -> Result<Value> {
    let content = get_pos_named!(args, content: EcoString);

    let max_consecutive_backticks = content
        .chars()
        .fold((0, 0), |(max, count), c| {
            if c == '`' {
                (max, count + 1)
            } else {
                (max.max(count), 0)
            }
        })
        .0;

    Ok(Value::Content(eco_format!(
        "{backticks}\n{content}\n{backticks}",
        backticks = "`".repeat((max_consecutive_backticks + 1).max(3)),
    )))
}

/// Evaluates a strike.
pub fn strike(mut args: Args) -> Result<Value> {
    let body = get_pos_named!(args, body: Content);

    Ok(Value::Content(eco_format!("~~{body}~~")))
}

/// Evaluates a padded content.
pub fn pad(mut args: Args) -> Result<Value> {
    Ok(get_pos_named!(args, path: Value))
}

/// Evaluates a `kbd` element.
pub fn kbd(mut args: Args) -> Result<Value> {
    let key = get_pos_named!(args, key: EcoString);

    Ok(Value::Content(eco_format!("<kbd>{key}</kbd>")))
}

/// Evaluates a markdown alteration.
pub fn md_alter(mut args: Args) -> Result<Value> {
    let _: () = get_pos_named!(args, left: ());
    let right = get_pos_named!(args, right: LazyContent);

    Ok(Value::Content(right.0))
}

/// Evaluates a note.
pub fn note(mut args: Args) -> Result<Value> {
    let body = get_pos_named!(args, body: Content);

    Ok(note_box("NOTE", body))
}

/// Evaluates a tip note box.
pub fn tip(mut args: Args) -> Result<Value> {
    let body = get_pos_named!(args, body: Content);

    Ok(note_box("TIP", body))
}

/// Creates a important note box.
pub fn important_box(mut args: Args) -> Result<Value> {
    let body = get_pos_named!(args, body: Content);

    Ok(note_box("IMPORTANT", body))
}

/// Creates a warning note box.
pub fn warning_box(mut args: Args) -> Result<Value> {
    let body = get_pos_named!(args, body: Content);

    Ok(note_box("WARNING", body))
}

/// Creates a caution note box.
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

/// Evaluates a table.
pub fn table(args: Args) -> Result<Value> {
    table_eval(args, &EcoString::from("table"))
}

/// Evaluates a grid.
pub fn grid(args: Args) -> Result<Value> {
    table_eval(args, &EcoString::from("grid"))
}

fn table_eval(mut args: Args, kind: &EcoString) -> Result<Value> {
    let columns = if let Some(columns) = args.get_named_("columns") {
        match columns.kind() {
            SyntaxKind::Array => {
                let array: ast::Array = args.get_named_("columns").unwrap().cast().unwrap();
                array.items().count()
            }
            SyntaxKind::Int => {
                let int_val: ast::Int = args.get_named_("columns").unwrap().cast().unwrap();
                int_val.get().try_into().unwrap()
            }
            other => return Err(format!("invalid columns argument of type {:?}", other).into()),
        }
    } else {
        1
    };

    let header_field = SyntaxNode::inner(
        SyntaxKind::FieldAccess,
        vec![
            SyntaxNode::leaf(SyntaxKind::Ident, kind),
            SyntaxNode::leaf(SyntaxKind::Dot, "."),
            SyntaxNode::leaf(SyntaxKind::Ident, "header"),
        ],
    );
    let footer_field = SyntaxNode::inner(
        SyntaxKind::FieldAccess,
        vec![
            SyntaxNode::leaf(SyntaxKind::Ident, kind),
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

    let mut res = EcoString::from("<table>\n");
    if !header.is_empty() {
        res.push_str("  <thead>\n    <tr>\n");
        for cell in &header {
            res.push_str(&eco_format!("      <th>{}</th>\n", cell));
        }
        res.push_str("    </tr>\n  </thead>\n");
    }
    res.push_str("  <tbody>\n");
    for row in cells.chunks(columns) {
        res.push_str("    <tr>\n");
        for cell in row {
            res.push_str(&eco_format!("      <td>{}</td>\n", cell));
        }
        res.push_str("    </tr>\n");
    }
    res.push_str("  </tbody>\n</table>");

    Ok(Value::Content(res))
}
