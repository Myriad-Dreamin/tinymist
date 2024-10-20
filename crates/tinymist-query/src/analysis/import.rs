//! Import analysis

use super::prelude::*;
use crate::analysis::analyze_import_;
use crate::syntax::{find_expr_in_import, resolve_id_by_path};

/// The import information of a source file.
#[derive(Default)]
pub struct ImportInfo {
    /// The source files that this source file depends on.
    pub deps: EcoVec<TypstFileId>,
    /// The source file that this source file imports.
    pub imports: indexmap::IndexMap<Range<usize>, Option<Source>>,
}

impl Hash for ImportInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(self.imports.len());
        // todo: import star is stateful
        for item in &self.imports {
            item.hash(state);
        }
    }
}

pub(super) fn get_import_info(
    ctx: comemo::Tracked<dyn World + '_>,
    source: Source,
    e: EcoVec<LexicalHierarchy>,
) -> Option<Arc<ImportInfo>> {
    let current_id = source.id();
    let root = LinkedNode::new(source.root());

    let mut collector = ImportCollector {
        ctx,
        info: ImportInfo::default(),

        current_id,
        root,
    };

    collector.scan(&e);

    let mut deps: Vec<_> = collector
        .info
        .imports
        .values()
        .filter_map(|x| x.as_ref().map(|x| x.id()))
        .collect();
    deps.sort();
    deps.dedup();
    collector.info.deps = deps.into();

    Some(Arc::new(collector.info))
}

struct ImportCollector<'a, 'w> {
    ctx: comemo::Tracked<'w, dyn World + 'w>,
    info: ImportInfo,

    current_id: TypstFileId,
    root: LinkedNode<'a>,
}

impl<'a, 'w> ImportCollector<'a, 'w> {
    fn scan(&mut self, e: &'a [LexicalHierarchy]) {
        for e in e {
            match &e.info.kind {
                LexicalKind::Heading(..) => unreachable!(),
                LexicalKind::Var(..) => {}
                LexicalKind::Block => {
                    if let Some(e) = &e.children {
                        self.scan(e.as_slice());
                    }
                }
                LexicalKind::Mod(
                    LexicalModKind::PathInclude
                    | LexicalModKind::PathVar
                    | LexicalModKind::ModuleAlias
                    | LexicalModKind::Ident
                    | LexicalModKind::Alias { .. }
                    | LexicalModKind::Star,
                ) => {}
                LexicalKind::Mod(LexicalModKind::Module(p)) => {
                    let id = match p {
                        ModSrc::Expr(exp) => {
                            let exp = self
                                .root
                                .leaf_at_compat(exp.range.end)
                                .and_then(find_expr_in_import);
                            let val = exp
                                .as_ref()
                                .and_then(|exp| analyze_import_(self.ctx.deref(), exp));

                            match val {
                                Some(Value::Module(m)) => {
                                    log::debug!(
                                        "current id {:?} exp {exp:?} => id: {:?}",
                                        self.current_id,
                                        m.file_id()
                                    );
                                    m.file_id()
                                }
                                Some(Value::Str(m)) => resolve_id_by_path(
                                    self.ctx.deref(),
                                    self.current_id,
                                    m.as_str(),
                                ),
                                _ => None,
                            }
                        }
                        ModSrc::Path(p) => {
                            resolve_id_by_path(self.ctx.deref(), self.current_id, p.deref())
                        }
                    };
                    log::debug!(
                        "current id {:?} range {:?} => id: {id:?}",
                        self.current_id,
                        e.info.range,
                    );
                    let source = id.and_then(|id| self.ctx.source(id).ok());
                    self.info.imports.insert(e.info.range.clone(), source);
                }
            }
        }
    }
}
