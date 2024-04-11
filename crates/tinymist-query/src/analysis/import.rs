//! Import analysis

use ecow::EcoVec;

use crate::syntax::find_source_by_import_path;

pub use super::prelude::*;

/// The import information of a source file.
#[derive(Default)]
pub struct ImportInfo {
    /// The source file that this source file imports.
    pub imports: indexmap::IndexMap<Range<usize>, Option<Source>>,
}

impl Hash for ImportInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(self.imports.len());
        for item in &self.imports {
            item.hash(state);
        }
    }
}

pub(super) fn get_import_info(
    ctx: &mut AnalysisContext,
    source: Source,
    e: EcoVec<LexicalHierarchy>,
) -> Option<Arc<ImportInfo>> {
    let current_id = source.id();

    let mut collector = ImportCollector {
        ctx,
        info: ImportInfo::default(),

        current_id,
    };

    collector.scan(&e);

    Some(Arc::new(collector.info))
}

struct ImportCollector<'a, 'w> {
    ctx: &'a mut AnalysisContext<'w>,
    info: ImportInfo,

    current_id: TypstFileId,
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
                LexicalKind::Mod(LexicalModKind::Module(p)) => match p {
                    ModSrc::Expr(_) => {}
                    ModSrc::Path(p) => {
                        let src = find_source_by_import_path(
                            self.ctx.world(),
                            self.current_id,
                            p.deref(),
                        );
                        self.info.imports.insert(e.info.range.clone(), src);
                    }
                },
            }
        }
    }
}
