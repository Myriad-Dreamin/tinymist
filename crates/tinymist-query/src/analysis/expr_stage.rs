//! Component-level scheduling for expression analysis.

use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use typst::{syntax::Source, utils::LazyHash};

use super::SharedContext;
use crate::syntax::{
    Decl, Expr, ExprInfo, LexicalScope, SelectExpr,
    expr::{ExportLookup, declaration_of, expr_of},
};

type DeclarationInterfaces = Arc<FxHashMap<typst::syntax::FileId, Arc<LazyHash<LexicalScope>>>>;

/// Immutable symbolic inputs for one declaration name-discovery round.
struct DeclarationInputs {
    interfaces: DeclarationInterfaces,
}

impl ExportLookup for DeclarationInputs {
    fn exports_of(
        &mut self,
        ctx: &Arc<SharedContext>,
        importer: typst::syntax::FileId,
        source: &Source,
    ) -> Option<Arc<LazyHash<LexicalScope>>> {
        self.interfaces
            .get(&source.id())
            .cloned()
            .or_else(|| ctx.external_exports_of(importer, source))
    }
}

/// Builds a complete component declaration batch without publishing a pending
/// member interface.
struct DeclarationBuilder {
    sources: Vec<Source>,
    inputs: DeclarationInputs,
}

impl DeclarationBuilder {
    fn new(sources: impl IntoIterator<Item = Source>) -> Self {
        let sources: Vec<_> = sources.into_iter().collect();
        let component: FxHashSet<_> = sources.iter().map(Source::id).collect();
        assert_eq!(
            component.len(),
            sources.len(),
            "an expression component cannot contain duplicate sources"
        );
        let empty_inputs = component
            .into_iter()
            .map(|fid| (fid, Arc::new(LazyHash::new(LexicalScope::default()))))
            .collect();
        Self {
            sources,
            inputs: DeclarationInputs {
                interfaces: Arc::new(empty_inputs),
            },
        }
    }

    fn finish(mut self, ctx: Arc<SharedContext>) -> DeclarationInterfaces {
        loop {
            // Keep this round's outputs local until every member has finished.
            // A declaration worker can only observe the preceding immutable
            // symbolic input batch.
            let mut outputs = FxHashMap::default();
            for source in &self.sources {
                let interface = declaration_of(ctx.clone(), source.clone(), &mut self.inputs);
                outputs.insert(source.id(), interface);
            }

            // The least declaration solution for a singleton's self-import
            // equation is its first local/explicit result. This also keeps the
            // ordinary acyclic-file path at one declaration pass.
            if self.sources.len() == 1 {
                return Arc::new(outputs);
            }

            let mut stable = true;
            for source in &self.sources {
                let fid = source.id();
                let input = self
                    .inputs
                    .interfaces
                    .get(&fid)
                    .expect("every declaration round must contain every component member");
                let output = outputs
                    .get(&fid)
                    .expect("every declaration round must produce every component member");
                assert!(
                    input.keys().all(|name| output.contains_key(name)),
                    "component declaration names must grow monotonically for {fid:?}"
                );
                stable &= input.keys().count() == output.keys().count();
            }

            if stable {
                return Arc::new(outputs);
            }

            // Only names flow between discovery rounds. Their values are
            // stable symbolic module selections, so mutually re-exported names
            // cannot copy and indefinitely wrap a preceding round's RefExpr.
            let symbolic_inputs = outputs
                .iter()
                .map(|(&fid, interface)| {
                    let module = Expr::Decl(Decl::module(fid).into());
                    let mut symbolic = LexicalScope::default();
                    for name in interface.keys() {
                        let field = Decl::lit_(name.clone()).into();
                        symbolic.insert_mut(
                            name.clone(),
                            Expr::Select(SelectExpr::new(field, module.clone())),
                        );
                    }
                    (fid, Arc::new(LazyHash::new(symbolic)))
                })
                .collect();
            self.inputs = DeclarationInputs {
                interfaces: Arc::new(symbolic_inputs),
            };
        }
    }
}

/// Tracks active and completed expression analyses for one PDG component.
pub(super) struct ExprStage {
    component: FxHashSet<typst::syntax::FileId>,
    declarations: DeclarationInterfaces,
    checking: FxHashSet<typst::syntax::FileId>,
    completed: FxHashMap<typst::syntax::FileId, ExprInfo>,
}

impl ExprStage {
    /// Creates a stage only after every declaration interface is sealed.
    pub(super) fn new(ctx: Arc<SharedContext>, sources: impl IntoIterator<Item = Source>) -> Self {
        let sources: Vec<_> = sources.into_iter().collect();
        let component: FxHashSet<_> = sources.iter().map(Source::id).collect();
        let declarations = DeclarationBuilder::new(sources).finish(ctx);
        assert!(
            declarations.len() == component.len()
                && declarations.keys().all(|fid| component.contains(fid)),
            "sealed declarations must cover the complete expression component"
        );
        Self {
            component,
            declarations,
            checking: Default::default(),
            completed: Default::default(),
        }
    }

    fn contains(&self, fid: &typst::syntax::FileId) -> bool {
        self.component.contains(fid)
    }

    /// The only call site allowed to enter [`expr_of`].
    pub(super) fn analyze(&mut self, ctx: Arc<SharedContext>, source: Source) -> ExprInfo {
        assert!(
            self.contains(&source.id()),
            "cross-component expression analysis must use the shared component entry point"
        );
        if let Some(cached) = self.completed.get(&source.id()) {
            return cached.clone();
        }
        assert!(
            self.checking.insert(source.id()),
            "recursive expression analysis must resolve through declared exports"
        );

        let guard = ctx.expr_stage_stat(source.id());
        guard.miss();
        let info = expr_of(ctx, source, self);
        self.checking.remove(&info.fid);
        self.completed.insert(info.fid, info.clone());
        info
    }
}

impl ExportLookup for ExprStage {
    fn exports_of(
        &mut self,
        ctx: &Arc<SharedContext>,
        importer: typst::syntax::FileId,
        source: &Source,
    ) -> Option<Arc<LazyHash<LexicalScope>>> {
        if !self.contains(&source.id()) {
            return ctx.external_exports_of(importer, source);
        }

        if let Some(info) = self.completed.get(&source.id()) {
            return Some(info.exports.clone());
        }
        if self.checking.contains(&source.id()) {
            return self.declarations.get(&source.id()).cloned();
        }
        Some(self.analyze(ctx.clone(), source.clone()).exports.clone())
    }
}
