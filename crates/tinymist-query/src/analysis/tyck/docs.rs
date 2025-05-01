use tinymist_analysis::docs::DocString;
use tinymist_std::TakeAs;

use super::*;

impl TypeChecker<'_> {
    pub fn check_docstring(&mut self, base_id: &Interned<Decl>) -> Option<Arc<DocString>> {
        let docstring = self.ei.docstrings.get(base_id)?.clone();
        Some(Arc::new(
            self.rename_based_on(docstring.take(), base_id.clone()),
        ))
    }

    fn rename_based_on(&mut self, docs: DocString, documenting_id: Interned<Decl>) -> DocString {
        let DocString {
            docs,
            var_bounds,
            vars,
            mut res_ty,
        } = docs;
        let mut renamer = IdRenamer {
            base: self,
            var_bounds: &var_bounds,
            base_id: documenting_id,
        };
        let mut vars = vars;
        for (_name, doc) in vars.iter_mut() {
            if let Some(ty) = &mut doc.ty {
                if let Some(mutated) = ty.mutate(true, &mut renamer) {
                    *ty = mutated;
                }
            }
        }
        if let Some(ty) = res_ty.as_mut() {
            if let Some(mutated) = ty.mutate(true, &mut renamer) {
                *ty = mutated;
            }
        }
        DocString {
            docs,
            var_bounds,
            vars,
            res_ty,
        }
    }
}

struct IdRenamer<'a, 'b> {
    base: &'a mut TypeChecker<'b>,
    var_bounds: &'a HashMap<DeclExpr, TypeVarBounds>,
    base_id: Interned<Decl>,
}

impl TyMutator for IdRenamer<'_, '_> {
    fn mutate(&mut self, ty: &Ty, pol: bool) -> Option<Ty> {
        match ty {
            Ty::Var(v) => Some(self.base.copy_doc_vars(
                self.var_bounds.get(&v.def).unwrap(),
                v,
                &self.base_id,
            )),
            ty => self.mutate_rec(ty, pol),
        }
    }
}
