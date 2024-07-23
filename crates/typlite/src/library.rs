use std::sync::OnceLock;

use ecow::eco_format;
use typst::{
    diag::{At, SourceResult},
    foundations::{Content, Dict, IntoValue, Module, NativeElement, Scope},
    introspection::MetadataElem,
    model::{Destination, LinkTarget},
    Library,
};
use typst_macros::func;

pub fn library() -> Library {
    let mut lib = Library::default();
    let mut global = Scope::new();
    // Copy from stdlib
    for (k, v) in lib.global.scope().iter() {
        global.define(k.clone(), v.clone());
    }
    global.define_func::<link>();

    lib.global = Module::new("global", global);

    lib
}

/// Evaluate a link to markdown-format string.
#[func]
pub fn link(dest: LinkTarget, body: Content) -> SourceResult<Content> {
    let mut dict = lite_elem();

    let dest = match dest {
        LinkTarget::Dest(Destination::Url(link)) => link,
        _ => return Err(eco_format!("unsupported link target")).at(body.span()),
    };
    let body = body.plain_text();

    dict.insert("raw".into(), eco_format!("[{body}]({dest})").into_value());
    Ok(MetadataElem::new(dict.into_value()).pack())
}

fn lite_elem() -> Dict {
    static ELEM_BASE: OnceLock<Dict> = OnceLock::new();

    ELEM_BASE
        .get_or_init(|| {
            let mut dict = Dict::new();
            dict.insert("$typlite".into(), true.into_value());

            dict
        })
        .clone()
}
