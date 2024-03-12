use typst::syntax::Source;

use super::{get_lexical_hierarchy, LexicalScopeKind};

pub fn get_def_use(source: Source) {
    let _ = get_lexical_hierarchy(source, LexicalScopeKind::DefUse);
}
