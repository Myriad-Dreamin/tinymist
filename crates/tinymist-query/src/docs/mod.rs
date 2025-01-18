//! Documentation utilities.

mod convert;
mod def;
mod module;
mod package;
mod tidy;

use tinymist_std::path::unix_slash;
use typst::syntax::FileId;

pub(crate) use convert::convert_docs;
pub use def::*;
pub use module::*;
pub use package::*;
pub(crate) use tidy::*;

fn file_id_repr(fid: FileId) -> String {
    if let Some(spec) = fid.package() {
        format!("{spec}{}", unix_slash(fid.vpath().as_rooted_path()))
    } else {
        unix_slash(fid.vpath().as_rooted_path())
    }
}
