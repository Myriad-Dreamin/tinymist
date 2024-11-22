//! Types and type operations for Typst.

mod apply;
mod bound;
mod builtin;
mod def;
mod describe;
mod iface;
mod mutate;
mod prelude;
mod select;
mod sig;
mod simplify;
mod subst;

pub(crate) use apply::*;
pub(crate) use bound::*;
pub(crate) use builtin::*;
pub use def::*;
pub(crate) use iface::*;
pub(crate) use mutate::*;
use reflexo_typst::TypstFileId;
pub(crate) use select::*;
pub(crate) use sig::*;
use typst::foundations::{self, Func, Module, Value};

/// A type context.
pub trait TyCtx {
    /// Get local binding of a variable.
    fn local_bind_of(&self, _var: &Interned<TypeVar>) -> Option<Ty>;
    /// Get the type of a variable.
    fn global_bounds(&self, _var: &Interned<TypeVar>, _pol: bool) -> Option<TypeBounds>;
}

impl TyCtx for () {
    fn local_bind_of(&self, _var: &Interned<TypeVar>) -> Option<Ty> {
        None
    }

    fn global_bounds(&self, _var: &Interned<TypeVar>, _pol: bool) -> Option<TypeBounds> {
        None
    }
}

/// A mutable type context.
pub trait TyCtxMut: TyCtx {
    /// The type of a snapshot of the scope.
    type Snap;

    /// Start a new scope.
    #[must_use]
    fn start_scope(&mut self) -> Self::Snap;
    /// End the current scope.
    fn end_scope(&mut self, snap: Self::Snap);
    /// Execute a function with a new scope.
    fn with_scope<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let snap = self.start_scope();
        let res = f(self);
        self.end_scope(snap);
        res
    }

    /// Bind a variable locally.
    fn bind_local(&mut self, var: &Interned<TypeVar>, ty: Ty);
    /// Get the type of a runtime function.
    fn type_of_func(&mut self, func: &Func) -> Option<Interned<SigTy>>;
    /// Get the type of a runtime value.
    fn type_of_value(&mut self, val: &Value) -> Ty;
    /// Get the type of a runtime dict.
    fn type_of_dict(&mut self, dict: &foundations::Dict) -> Interned<RecordTy> {
        let ty = self.type_of_value(&Value::Dict(dict.clone()));
        let Ty::Dict(ty) = ty else {
            panic!("expected dict type, found {ty:?}");
        };
        ty
    }
    /// Get the type of a runtime module.
    fn type_of_module(&mut self, module: &Module) -> Interned<RecordTy> {
        let ty = self.type_of_value(&Value::Module(module.clone()));
        let Ty::Dict(ty) = ty else {
            panic!("expected dict type, found {ty:?}");
        };
        ty
    }
    /// Check a module item.
    fn check_module_item(&mut self, module: TypstFileId, key: &StrRef) -> Option<Ty>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adt::interner::Interned;
    use crate::syntax::Decl;

    pub fn var_ins(s: &str) -> Ty {
        Ty::Var(TypeVar::new(s.into(), Decl::lit(s).into()))
    }

    pub fn str_sig(
        pos: &[&str],
        named: &[(&str, &str)],
        rest: Option<&str>,
        ret: Option<&str>,
    ) -> Interned<SigTy> {
        let pos = pos.iter().map(|s| var_ins(s));
        let named = named.iter().map(|(n, t)| ((*n).into(), var_ins(t)));
        let rest = rest.map(var_ins);
        let ret = ret.map(var_ins);
        SigTy::new(pos, named, None, rest, ret).into()
    }

    // args*, (keys: values)*, ...rest -> ret
    macro_rules! literal_sig {
        ($($pos:ident),* $(!$named:ident: $named_ty:ident),* $(,)? ...$rest:ident -> $ret:ident) => {
            str_sig(&[$(stringify!($pos)),*], &[$((stringify!($named), stringify!($named_ty))),*], Some(stringify!($rest)), Some(stringify!($ret)))
        };
        ($($pos:ident),* $(!$named:ident: $named_ty:ident),* $(,)? -> $ret:ident) => {
            str_sig(&[$(stringify!($pos)),*], &[$((stringify!($named), stringify!($named_ty))),*], None, Some(stringify!($ret)))
        };
        ($($pos:ident),* $(!$named:ident: $named_ty:ident),* $(,)? ...$rest:ident) => {
            str_sig(&[$(stringify!($pos)),*], &[$((stringify!($named), stringify!($named_ty))),*], Some(stringify!($rest)), None)
        };
        ($($pos:ident),* $(!$named:ident: $named_ty:ident),* $(,)?) => {
            str_sig(&[$(stringify!($pos)),*], &[$((stringify!($named), stringify!($named_ty))),*], None, None)
        };
    }

    pub(crate) use literal_sig;
    pub(crate) use literal_sig as literal_args;
}
