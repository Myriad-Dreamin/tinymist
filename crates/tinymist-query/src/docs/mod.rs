//! Documentation generation utilities.

mod docstring;
mod library;
mod module;
mod package;
mod signature;
mod tidy;

use ecow::EcoString;
use reflexo::path::unix_slash;
use typst::{foundations::Value, syntax::FileId};

pub use docstring::*;
pub use module::*;
pub use package::*;
pub use signature::*;
pub(crate) use tidy::*;

fn file_id_repr(k: FileId) -> String {
    if let Some(p) = k.package() {
        format!("{p}{}", unix_slash(k.vpath().as_rooted_path()))
    } else {
        unix_slash(k.vpath().as_rooted_path())
    }
}

fn kind_of(val: &Value) -> EcoString {
    match val {
        Value::Module(_) => "module",
        Value::Type(_) => "struct",
        Value::Func(_) => "function",
        Value::Label(_) => "reference",
        _ => "constant",
    }
    .into()
}
