use core::fmt;
use std::path::PathBuf;

use tinymist_project::LspWorld;
use typst::syntax::Source;

use crate::ty::{Ty, TypeInfo};

pub fn snapshot_testing(name: &str, f: &impl Fn(LspWorld, PathBuf)) {
    tinymist_tests::snapshot_testing!(name, |verse, path| {
        f(verse.snapshot(), path);
    });
}

pub struct TypeCheckSnapshot<'a>(pub &'a Source, pub &'a TypeInfo);

impl fmt::Debug for TypeCheckSnapshot<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let source = self.0;
        let info = self.1;
        let mut vars = info
            .vars
            .values()
            .map(|bounds| (bounds.name(), bounds))
            .collect::<Vec<_>>();

        vars.sort_by(|x, y| x.1.var.strict_cmp(&y.1.var));

        for (name, bounds) in vars {
            writeln!(f, "{name:?} = {:?}", info.simplify(bounds.as_type(), true))?;
        }

        writeln!(f, "=====")?;
        let mut mapping = info
            .mapping
            .iter()
            .map(|pair| (source.range(*pair.0).unwrap_or_default(), pair.1))
            .collect::<Vec<_>>();

        mapping.sort_by(|x, y| {
            x.0.start
                .cmp(&y.0.start)
                .then_with(|| x.0.end.cmp(&y.0.end))
        });

        for (range, value) in mapping {
            let ty = Ty::from_types(value.clone().into_iter());
            writeln!(f, "{range:?} -> {ty:?}")?;
        }

        Ok(())
    }
}
