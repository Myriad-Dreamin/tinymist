use super::{Sig, SigShape, TyMutator};
use crate::ty::prelude::*;

impl Sig<'_> {
    /// Calls the signature with the given arguments.
    pub fn call(&self, args: &Interned<ArgsTy>, pol: bool, ctx: &mut impl TyCtxMut) -> Option<Ty> {
        crate::log_debug_ct!("call {self:?} {args:?} {pol:?}");
        ctx.with_scope(|ctx| {
            let body = self.check_bind(args, ctx)?;

            // Substitute the bound variables in the body or just body
            let mut checker = SubstituteChecker { ctx };
            Some(checker.ty(&body, pol).unwrap_or(body))
        })
    }

    /// Checks the binding of the signature.
    pub fn check_bind(&self, args: &Interned<ArgsTy>, ctx: &mut impl TyCtxMut) -> Option<Ty> {
        let SigShape { sig, withs } = self.shape(ctx)?;

        // todo: check if the signature has free variables
        // let has_free_vars = sig.has_free_variables;

        let rest_bind = Self::rest_bind(&sig, args, withs);

        for (arg_recv, arg_ins) in sig.matches(args, withs) {
            if let Ty::Var(arg_recv) = arg_recv {
                crate::log_debug_ct!("bind {arg_recv:?} {arg_ins:?}");
                ctx.bind_local(arg_recv, arg_ins.clone());
            }
        }

        if let Some((rest_var, rest_ty)) = rest_bind {
            crate::log_debug_ct!("bind rest {rest_var:?} {rest_ty:?}");
            ctx.bind_local(&rest_var, rest_ty);
        }

        sig.body.clone()
    }

    fn rest_bind(
        sig: &Interned<SigTy>,
        args: &Interned<ArgsTy>,
        withs: Option<&Vec<Interned<SigTy>>>,
    ) -> Option<(Interned<TypeVar>, Ty)> {
        let Ty::Var(rest_var) = sig.rest_param()? else {
            return None;
        };

        let fixed_pos = sig.positional_params().len();
        let rest_pos = withs
            .into_iter()
            .flat_map(|withs| withs.iter().rev())
            .flat_map(|with| with.positional_params())
            .chain(args.positional_params())
            .skip(fixed_pos)
            .cloned()
            .collect::<Vec<_>>();

        let rest_named = args
            .named_params()
            .filter(|(name, _)| sig.named(name).is_none())
            .map(|(name, ty)| (name.clone(), ty.clone()))
            .collect::<Vec<_>>();

        let rest = args.rest_param().cloned();
        let rest_args = ArgsTy::new(rest_pos.into_iter(), rest_named, None, rest, None);

        Some((rest_var.clone(), Ty::Args(rest_args.into())))
    }
}

/// A checker to substitute the bound variables.
struct SubstituteChecker<'a, T: TyCtxMut> {
    ctx: &'a mut T,
}

impl<T: TyCtxMut> SubstituteChecker<'_, T> {
    /// Substitutes the bound variables in the given type.
    fn ty(&mut self, body: &Ty, pol: bool) -> Option<Ty> {
        body.mutate(pol, self)
    }
}

impl<T: TyCtxMut> TyMutator for SubstituteChecker<'_, T> {
    fn mutate(&mut self, ty: &Ty, pol: bool) -> Option<Ty> {
        // todo: extrude the type into a polarized type
        match ty {
            Ty::Var(var) => self.ctx.local_bind_of(var),
            Ty::Let(bounds) => {
                let mut lbs = bounds
                    .lbs
                    .iter()
                    .map(|bound| self.mutate(bound, !pol).unwrap_or_else(|| bound.clone()))
                    .collect::<Vec<_>>();
                let mut ubs = bounds
                    .ubs
                    .iter()
                    .map(|bound| self.mutate(bound, pol).unwrap_or_else(|| bound.clone()))
                    .collect::<Vec<_>>();
                if ubs.is_empty() && lbs.len() == 1 {
                    return lbs.pop();
                }
                if lbs.is_empty() && ubs.len() == 1 {
                    return ubs.pop();
                }
                Some(Ty::Let(TypeBounds { lbs, ubs }.into()))
            }
            _ => self.mutate_rec(ty, pol),
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::{assert_debug_snapshot, assert_snapshot};
    use tinymist_derive::BindTyCtx;

    use super::{DynTypeBounds, Interned, Ty, TyCtx, TypeInfo, TypeVar};
    use crate::ty::ApplyChecker;
    use crate::ty::tests::*;
    #[test]
    fn test_ty() {
        use super::*;
        let ty = Ty::Builtin(BuiltinTy::Clause);
        let ty_ref = TyRef::new(ty.clone());
        assert_debug_snapshot!(ty_ref, @"Clause");
    }

    #[derive(Default, BindTyCtx)]
    #[bind(0)]
    struct CallCollector(TypeInfo, Vec<Ty>);

    impl ApplyChecker for CallCollector {
        fn apply(
            &mut self,
            sig: super::Sig,
            arguments: &crate::adt::interner::Interned<super::ArgsTy>,
            pol: bool,
        ) {
            let ty = sig.call(arguments, pol, &mut self.0);
            if let Some(ty) = ty {
                self.1.push(ty);
            }
        }
    }

    #[test]
    fn test_sig_call() {
        use super::*;

        fn call(sig: Interned<SigTy>, args: Interned<SigTy>) -> String {
            let sig_ty = Ty::Func(sig);
            let mut collector = CallCollector::default();
            sig_ty.call(&args, false, &mut collector);

            collector.1.iter().fold(String::new(), |mut acc, ty| {
                if !acc.is_empty() {
                    acc.push_str(", ");
                }

                acc.push_str(&format!("{ty:?}"));
                acc
            })
        }

        assert_snapshot!(call(literal_sig!(p1 -> p1), literal_args!(q1)), @"@q1");
        assert_snapshot!(call(literal_sig!(!u1: w1 -> w1), literal_args!(!u1: w2)), @"@w2");
    }
}
