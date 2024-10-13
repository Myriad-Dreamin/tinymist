//! Types and type operations for Typst.

mod apply;
mod bound;
mod builtin;
mod def;
mod describe;
mod iface;
mod mutate;
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
pub(crate) use select::*;
pub(crate) use sig::*;

use crate::adt::interner::Interned;

/// A type context.
pub trait TyCtx {
    /// Get the type of a variable.
    fn var_bounds(&self, _var: &Interned<TypeVar>, _pol: bool) -> (Option<Ty>, Option<TypeBounds>) {
        (None, None)
    }
}

impl TyCtx for () {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adt::interner::Interned;
    use reflexo::vector::ir::DefId;
    use rustc_hash::FxHasher;
    use std::hash::{Hash, Hasher};

    /// A convenience function for when you need a quick 64-bit hash.
    #[inline]
    pub fn hash64<T: Hash + ?Sized>(v: &T) -> u64 {
        let mut state = FxHasher::default();
        v.hash(&mut state);
        state.finish()
    }

    pub fn var_ins(s: &str) -> Ty {
        Ty::Var(TypeVar::new(s.into(), DefId(hash64(s))))
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
        SigTy::new(pos, named, rest, ret).into()
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
