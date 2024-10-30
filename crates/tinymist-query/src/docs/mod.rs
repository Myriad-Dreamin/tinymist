//! Documentation generation utilities.

mod library;
mod module;
mod package;
mod symbol;
mod tidy;

use reflexo::path::unix_slash;
use typst::syntax::FileId;

pub use module::*;
pub use package::*;
pub use symbol::*;
pub(crate) use tidy::*;

fn file_id_repr(k: FileId) -> String {
    if let Some(p) = k.package() {
        format!("{p}{}", unix_slash(k.vpath().as_rooted_path()))
    } else {
        unix_slash(k.vpath().as_rooted_path())
    }
}
