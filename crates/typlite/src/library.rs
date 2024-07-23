use super::*;
use ecow::eco_format;
use value::{get_pos_named, RawFunc};

pub fn library() -> Scopes<Value> {
    let mut scopes = Scopes::new();
    scopes.define("link", link as RawFunc);
    scopes
}

/// Evaluate a link to markdown-format string.
pub fn link(mut args: ArgGetter) -> Result<Value> {
    let dest = get_pos_named!(args, dest: EcoString);
    let body = get_pos_named!(args, body: &SyntaxNode);
    let body = args.worker.convert(body)?;

    Ok(Value::Content(eco_format!("[{body}]({dest})")))
}
