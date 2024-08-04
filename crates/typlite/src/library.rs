use super::*;
use ecow::eco_format;
use value::*;

pub fn library() -> Scopes<Value> {
    let mut scopes = Scopes::new();
    scopes.define("link", link as RawFunc);
    scopes.define("kbd", kbd as RawFunc);
    // todo: how to import this function correctly?
    scopes.define("cross-link", cross_link as RawFunc);
    scopes.define("md-alter", md_alter as RawFunc);
    scopes.define("image", image as RawFunc);
    scopes.define("figure", figure as RawFunc);
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

/// Evaluate a `kbd` element.
pub fn kbd(mut args: Args) -> Result<Value> {
    let key = get_pos_named!(args, key: EcoString);

    Ok(Value::Content(eco_format!("<kbd>{key}</kbd>")))
}

/// Evaluate a `cross-link`.
pub fn cross_link(mut args: Args) -> Result<Value> {
    let dest = get_pos_named!(args, dest: EcoString);
    let body = get_pos_named!(args, body: Content);

    let dest = std::path::Path::new(dest.as_str()).with_extension("html");

    Ok(Value::Content(eco_format!(
        "[{body}](https://myriad-dreamin.github.io/tinymist/{dest})",
        dest = dest.to_string_lossy()
    )))
}

/// Evaluate a markdown alteration.
pub fn md_alter(mut args: Args) -> Result<Value> {
    let _left = get_pos_named!(args, left: Content);
    let right = get_pos_named!(args, right: LazyContent);

    Ok(Value::Content(right.0))
}
