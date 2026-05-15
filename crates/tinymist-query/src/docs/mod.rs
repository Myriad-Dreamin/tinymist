//! Documentation utilities.

mod convert;
mod def;
mod module;
mod package;

use std::path::Path;
use tinymist_std::path::unix_slash;
use typst::syntax::FileId;

pub(crate) use convert::convert_docs;
pub(crate) use def::*;
pub use module::*;
pub use package::*;
pub use tinymist_analysis::docs::*;

fn file_id_repr(fid: FileId) -> String {
    if let typst::syntax::VirtualRoot::Package(spec) = fid.root() {
        format!(
            "{spec}{}",
            unix_slash(Path::new(fid.vpath().get_with_slash()))
        )
    } else {
        unix_slash(Path::new(fid.vpath().get_with_slash()))
    }
}
