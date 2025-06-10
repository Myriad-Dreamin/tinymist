use core::fmt;
use std::path::PathBuf;

use tinymist_project::LspWorld;

use crate::ty::TypeInfo;

pub fn snapshot_testing(name: &str, f: &impl Fn(LspWorld, PathBuf)) {
    tinymist_tests::snapshot_testing!(name, |verse, path| {
        f(verse.snapshot(), path);
    });
}

pub struct TypeCheckSnapshot<'a>(pub &'a TypeInfo);

impl fmt::Debug for TypeCheckSnapshot<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let info = self.0;
        let mut exports = info.exports.iter().collect::<Vec<_>>();

        exports.sort_by(|x, y| x.0.cmp(y.0));

        for (name, bounds) in exports {
            writeln!(f, "{name:?} = {:?}", info.simplify(bounds.clone(), true))?;
        }

        Ok(())
    }
}
