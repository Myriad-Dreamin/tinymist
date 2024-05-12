//! Types and type operations for Typst.

mod apply;
mod bound;
mod builtin;
mod def;
mod describe;
mod sig;
mod simplify;
mod subst;

pub(crate) use apply::*;
pub(crate) use bound::*;
pub(crate) use builtin::*;
pub(crate) use def::*;
pub(crate) use sig::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adt::interner::Interned;
    use typst::foundations::Value;

    pub fn str_ins(s: &str) -> Ty {
        Ty::Value(InsTy::new(Value::Str(s.into())))
    }

    pub fn str_sig(
        pos: &[&str],
        named: &[(&str, &str)],
        rest: Option<&str>,
        ret: Option<&str>,
    ) -> Interned<SigTy> {
        let pos = pos.iter().map(|s| str_ins(s));
        let named = named.iter().map(|(n, t)| ((*n).into(), str_ins(t)));
        let rest = rest.map(str_ins);
        let ret = ret.map(str_ins);
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
}
