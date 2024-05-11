use crate::{adt::interner::Interned, ty::def::*};

pub trait BoundChecker {
    fn collect(&mut self, ty: &Ty, pol: bool);
    fn bound_of_var(&mut self, _var: &Interned<TypeVar>, _pol: bool) -> Option<TypeBounds> {
        None
    }
}

impl<T> BoundChecker for T
where
    T: FnMut(&Ty, bool) -> Option<TypeBounds>,
{
    fn collect(&mut self, ty: &Ty, pol: bool) {
        self(ty, pol);
    }
}

impl Ty {
    pub fn has_bounds(&self) -> bool {
        matches!(self, Ty::Union(_) | Ty::Let(_) | Ty::Var(_))
    }

    pub fn bounds(&self, pol: bool, checker: &mut impl BoundChecker) {
        let mut worker = BoundCheckContext;
        worker.ty(self, pol, checker);
    }
}

pub struct BoundCheckContext;

impl BoundCheckContext {
    fn tys<'a>(
        &mut self,
        tys: impl Iterator<Item = &'a Ty>,
        pol: bool,
        checker: &mut impl BoundChecker,
    ) {
        for ty in tys {
            self.ty(ty, pol, checker);
        }
    }

    fn ty(&mut self, ty: &Ty, pol: bool, checker: &mut impl BoundChecker) {
        match ty {
            Ty::Union(u) => {
                self.tys(u.iter(), pol, checker);
            }
            Ty::Let(u) => {
                self.tys(u.ubs.iter(), pol, checker);
                self.tys(u.lbs.iter(), !pol, checker);
            }
            Ty::Var(u) => {
                let Some(w) = checker.bound_of_var(u, pol) else {
                    return;
                };
                self.tys(w.ubs.iter(), pol, checker);
                self.tys(w.lbs.iter(), !pol, checker);
            }
            // todo: calculate these operators
            // Ty::Select(_) => {}
            // Ty::Unary(_) => {}
            // Ty::Binary(_) => {}
            // Ty::If(_) => {}
            ty => checker.collect(ty, pol),
        }
    }
}
