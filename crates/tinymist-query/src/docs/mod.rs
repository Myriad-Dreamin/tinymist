//! Documentation utilities.

mod convert;
mod def;
mod module;
mod package;

use tinymist_std::path::unix_slash;
use typst::syntax::FileId;
use typst_shim::syntax::VirtualPathExt;

pub(crate) use convert::convert_docs;
pub(crate) use def::*;
pub use module::*;
pub use package::*;
pub use tinymist_analysis::docs::*;

fn file_id_repr(fid: FileId) -> String {
    if let typst::syntax::VirtualRoot::Package(spec) = fid.root() {
        format!(
            "{spec}{}",
            unix_slash(fid.vpath().as_rooted_path_compat())
        )
    } else {
        unix_slash(fid.vpath().as_rooted_path_compat())
    }
}
