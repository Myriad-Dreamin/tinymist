use std::ops::Deref;

use typst::foundations::{self, Func};

use crate::syntax::DeclExpr;
use crate::ty::prelude::*;

/// A trait for checking the bounds of a type.
pub trait BoundChecker: Sized + TyCtx {
    /// Collects the bounds of a type.
    fn collect(&mut self, ty: &Ty, pol: bool);

    /// Checks the bounds of a variable.
    fn check_var(&mut self, u: &Interned<TypeVar>, pol: bool, ctx: &mut BoundCheckContext) {
        ctx.check_var_rec(u, pol, self);
    }

    /// Checks the bounds of a variable recursively.
    fn check_var_rec(&mut self, u: &Interned<TypeVar>, pol: bool) {
        let mut ctx = BoundCheckContext::default();
        ctx.check_var_rec(u, pol, self);
    }
}

/// A predicate for checking the bounds of a type.
#[derive(BindTyCtx)]
#[bind(0)]
pub struct BoundPred<'a, T: TyCtx, F>(pub &'a T, pub F);

impl<'a, T: TyCtx, F> BoundPred<'a, T, F> {
    /// Creates a new bound predicate.
    pub fn new(t: &'a T, f: F) -> Self {
        Self(t, f)
    }
}

impl<T: TyCtx, F> BoundChecker for BoundPred<'_, T, F>
where
    F: FnMut(&Ty, bool),
{
    fn collect(&mut self, ty: &Ty, pol: bool) {
        self.1(ty, pol);
    }
}

/// A source of documentation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DocSource {
    /// A variable source.
    Var(Interned<TypeVar>),
    /// An (value) instance source.
    Ins(Interned<InsTy>),
    /// A builtin type source.
    Builtin(BuiltinTy),
}

impl DocSource {
    /// Casts doc source to a function.
    pub fn as_func(&self) -> Option<Func> {
        match self {
            Self::Var(..) => None,
            Self::Builtin(BuiltinTy::Type(ty)) => Some(ty.constructor().ok()?),
            Self::Builtin(BuiltinTy::Element(ty)) => Some((*ty).into()),
            Self::Builtin(..) => None,
            Self::Ins(ins_ty) => match &ins_ty.val {
                foundations::Value::Func(func) => Some(func.clone()),
                foundations::Value::Type(ty) => Some(ty.constructor().ok()?),
                _ => None,
            },
        }
    }
}

impl Ty {
    /// Checks if the given type has bounds (is combinated).
    pub fn has_bounds(&self) -> bool {
        matches!(self, Ty::Union(_) | Ty::Let(_) | Ty::Var(_))
    }

    /// Converts a type to doc source.
    pub fn as_source(&self) -> Option<DocSource> {
        match self {
            Ty::Builtin(ty @ (BuiltinTy::Type(..) | BuiltinTy::Element(..))) => {
                Some(DocSource::Builtin(ty.clone()))
            }
            Ty::Value(ty) => match &ty.val {
                foundations::Value::Type(..) | foundations::Value::Func(..) => {
                    Some(DocSource::Ins(ty.clone()))
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Gets the sources of the given type.
    pub fn sources(&self) -> Vec<DocSource> {
        let mut results = vec![];
        fn collect(ty: &Ty, results: &mut Vec<DocSource>) {
            use Ty::*;
            if let Some(src) = ty.as_source() {
                results.push(src);
                return;
            }
            match ty {
                Any | Boolean(_) | If(..) | Builtin(..) | Value(..) => {}
                Dict(..) | Array(..) | Tuple(..) | Func(..) | Args(..) | Pattern(..) => {}
                Unary(..) | Binary(..) => {}
                Param(ty) => {
                    // todo: doc source can be param ty
                    collect(&ty.ty, results);
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
                    results.push(DocSource::Var(ty.clone()));
                }
                With(ty) => collect(&ty.sig, results),
                Select(ty) => {
                    // todo: do this correctly
                    if matches!(ty.select.deref(), "with" | "where") {
                        collect(&ty.ty, results);
                    }

                    // collect(&ty.ty, results)
                }
            }
        }

        collect(self, &mut results);
        results
    }

    /// Profiles the bounds of the given type.
    pub fn bounds(&self, pol: bool, checker: &mut impl BoundChecker) {
        let mut ctx = BoundCheckContext::default();
        ctx.ty(self, pol, checker);
    }
}

/// A context for checking the bounds of a type.
#[derive(Default)]
pub struct BoundCheckContext {
    visiting: FxHashSet<(DeclExpr, bool)>,
    steps: usize,
}

impl BoundCheckContext {
    const STEP_BUDGET: usize = 100_000;

    fn enter(&mut self) -> bool {
        self.steps += 1;
        self.steps <= Self::STEP_BUDGET
    }

    /// Checks the bounds of multiple types.
    fn tys<'a>(&mut self, tys: impl Iterator<Item = &'a Ty>, pol: bool, c: &mut impl BoundChecker) {
        for ty in tys {
            self.ty(ty, pol, c);
        }
    }

    /// Recursively checks a variable while preserving this traversal's cycle guard.
    pub fn check_var_rec(
        &mut self,
        u: &Interned<TypeVar>,
        pol: bool,
        checker: &mut impl BoundChecker,
    ) {
        if !self.enter() {
            return;
        }

        let key = (u.def.clone(), pol);
        if !self.visiting.insert(key.clone()) {
            return;
        }

        if let Some(w) = checker.global_bounds(u, pol) {
            self.tys(w.ubs.iter(), pol, checker);
            self.tys(w.lbs.iter(), !pol, checker);
        }

        self.visiting.remove(&key);
    }

    /// Checks the bounds of a type.
    fn ty(&mut self, ty: &Ty, pol: bool, checker: &mut impl BoundChecker) {
        if !self.enter() {
            return;
        }

        match ty {
            Ty::Union(u) => {
                self.tys(u.iter(), pol, checker);
            }
            Ty::Let(u) => {
                self.tys(u.ubs.iter(), pol, checker);
                self.tys(u.lbs.iter(), !pol, checker);
            }
            Ty::Var(u) => checker.check_var(u, pol, self),
            // todo: calculate these operators
            // Ty::Select(_) => {}
            // Ty::Unary(_) => {}
            // Ty::Binary(_) => {}
            // Ty::If(_) => {}
            ty => checker.collect(ty, pol),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::Decl;

    struct HookChecker {
        bounds: FxHashMap<DeclExpr, DynTypeBounds>,
        hooks: Vec<DeclExpr>,
    }

    impl TyCtx for HookChecker {
        fn local_bind_of(&self, _var: &Interned<TypeVar>) -> Option<Ty> {
            None
        }

        fn global_bounds(&self, var: &Interned<TypeVar>, _pol: bool) -> Option<DynTypeBounds> {
            self.bounds.get(&var.def).cloned()
        }
    }

    impl BoundChecker for HookChecker {
        fn collect(&mut self, _ty: &Ty, _pol: bool) {}

        fn check_var(&mut self, var: &Interned<TypeVar>, pol: bool, ctx: &mut BoundCheckContext) {
            self.hooks.push(var.def.clone());
            ctx.check_var_rec(var, pol, self);
        }
    }

    #[test]
    fn custom_var_hook_preserves_cycle_guard() {
        let a = TypeVar::new("a".into(), Decl::lit("a").into());
        let b = TypeVar::new("b".into(), Decl::lit("b").into());

        let mut a_bounds = DynTypeBounds::default();
        a_bounds.ubs.insert_mut(Ty::Var(b.clone()));
        let mut b_bounds = DynTypeBounds::default();
        b_bounds.ubs.insert_mut(Ty::Var(a.clone()));

        let mut checker = HookChecker {
            bounds: [(a.def.clone(), a_bounds), (b.def.clone(), b_bounds)]
                .into_iter()
                .collect(),
            hooks: vec![],
        };

        Ty::Var(a.clone()).bounds(true, &mut checker);

        assert_eq!(
            checker.hooks,
            vec![a.def.clone(), b.def.clone(), a.def.clone()]
        );
    }
}
