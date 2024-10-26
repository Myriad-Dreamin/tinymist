use reflexo::TakeAs;

use super::*;
use crate::syntax::DocString;

impl<'a> TypeChecker<'a> {
    pub fn check_docstring(&mut self, base_id: &Interned<Decl>) -> Option<Arc<DocString>> {
        let docstring = self.ei.docstrings.get(base_id)?.clone();
        Some(Arc::new(
            docstring.take().rename_based_on(base_id.clone(), self),
        ))
    }
}

impl DocString {
    fn rename_based_on(self, documenting_id: Interned<Decl>, base: &mut TypeChecker) -> DocString {
        let DocString {
            docs,
            var_bounds,
            vars,
            mut res_ty,
        } = self;
        let mut renamer = IdRenamer {
            base,
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

impl<'a, 'b> TyMutator for IdRenamer<'a, 'b> {
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
