use std::sync::Arc;

use ecow::eco_format;
use typlite::value::*;

pub(super) fn lib() -> Arc<typlite::scopes::Scopes<Value>> {
    let mut scopes = typlite::library::library();

    // todo: how to import this function correctly?
    scopes.define("example", example as RawFunc);

    Arc::new(scopes)
}

/// Evaluate a `example`.
pub fn example(mut args: Args) -> typlite::Result<Value> {
    let body = get_pos_named!(args, body: Content).0;
    let body = body.trim();
    let ticks = body.chars().take_while(|t| *t == '`').collect::<String>();
    let body = &body[ticks.len()..];
    let body = eco_format!("{ticks}typ{body}");

    Ok(Value::Content(body))
}
