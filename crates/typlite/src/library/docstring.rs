use super::*;

pub fn docstring_lib() -> Scopes<Value> {
    let mut scopes = library();

    scopes.define("example", example as RawFunc);

    scopes
}

/// Evaluate a `example`.
pub fn example(mut args: Args) -> Result<Value> {
    let body = get_pos_named!(args, body: &SyntaxNode);
    let body = body
        .cast::<ast::Raw>()
        .ok_or_else(|| format!("expected raw, found {:?}", body.kind()))?;

    let lang = body.lang().map(|l| l.get().as_str()).unwrap_or("typ");

    // Handle example docs specially.
    // <https://github.com/typst/typst/blob/070e3144b33e9a9e9839c138df2b0a13dde7abc7/docs/src/html.rs#L355>
    let mut display = String::new();
    let mut compile = String::new();
    for line in body.lines() {
        let line = line.get();
        if let Some(suffix) = line.strip_prefix(">>>") {
            compile.push_str(suffix);
            compile.push('\n');
        } else if let Some(suffix) = line.strip_prefix("<<< ") {
            display.push_str(suffix);
            display.push('\n');
        } else {
            display.push_str(line);
            display.push('\n');
            compile.push_str(line);
            compile.push('\n');
        }
    }

    let mut s = EcoString::new();

    s.push_str("```");
    s.push_str(lang);
    s.push('\n');
    s.push_str(&display);
    s.push('\n');
    s.push_str("```");
    s.push('\n');

    // todo: render examples only if supports HTML
    let is_code = lang == "typc";
    let rendered = args
        .vm
        .render_code(&compile, !is_code, "left", r#"width="500px""#, false)?;
    s.push_str(&TypliteWorker::value(rendered));

    Ok(Value::Content(s))
}
