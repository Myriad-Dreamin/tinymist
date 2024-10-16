use typst::foundations;

use crate::ty::prelude::*;

pub trait BoundChecker: TyCtx {
    fn collect(&mut self, ty: &Ty, pol: bool);
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeSources {
    Var(Interned<TypeVar>),
    Ins(Interned<InsTy>),
    Builtin(BuiltinTy),
}

impl Ty {
    /// Check if the given type has bounds (is combinated).
    pub fn has_bounds(&self) -> bool {
        matches!(self, Ty::Union(_) | Ty::Let(_) | Ty::Var(_))
    }

    /// Get the sources of the given type.
    pub fn sources(&self) -> Vec<TypeSources> {
        let mut results = vec![];
        fn collect(ty: &Ty, results: &mut Vec<TypeSources>) {
            use Ty::*;
            match ty {
                Any | Boolean(_) | If(..) => {}
                Dict(..) | Array(..) | Tuple(..) | Func(..) | Args(..) => {}
                Unary(..) | Binary(..) => {}
                Field(ty) => {
                    collect(&ty.field, results);
                }
                Union(ty) => {
                    for ty in ty.iter() {
                        collect(ty, results);
                    }
                }
                Let(ty) => {
                    for ty in ty.ubs.iter() {
                        collect(ty, results);
                    }
                    for ty in ty.lbs.iter() {
                        collect(ty, results);
                    }
                }
                Var(ty) => {
                    results.push(TypeSources::Var(ty.clone()));
                }
                Builtin(ty @ (BuiltinTy::Type(..) | BuiltinTy::Element(..))) => {
                    results.push(TypeSources::Builtin(ty.clone()));
                }
                Builtin(..) => {}
                Value(ty) => match &ty.val {
                    foundations::Value::Type(..) | foundations::Value::Func(..) => {
                        results.push(TypeSources::Ins(ty.clone()));
                    }
                    _ => {}
                },
                With(ty) => collect(&ty.sig, results),
                Select(ty) => collect(&ty.ty, results),
            }
        }

        collect(self, &mut results);
        results
    }

    /// Profile the bounds of the given type.
    pub fn bounds(&self, pol: bool, checker: &mut impl BoundChecker) {
        BoundCheckContext.ty(self, pol, checker);
    }
}

pub struct BoundCheckContext;

impl BoundCheckContext {
    fn tys<'a>(&mut self, tys: impl Iterator<Item = &'a Ty>, pol: bool, c: &mut impl BoundChecker) {
        for ty in tys {
            self.ty(ty, pol, c);
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
                let Some(w) = checker.global_bounds(u, pol) else {
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
