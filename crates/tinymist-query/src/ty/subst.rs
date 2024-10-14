use crate::{analysis::*, ty::def::*};

impl<'a> Sig<'a> {
    pub fn call(
        &self,
        args: &Interned<ArgsTy>,
        pol: bool,
        ctx: &mut impl TyCtxMut,
    ) -> Option<Ty> {
        log::debug!("call {self:?} {args:?} {pol:?}");
        ctx.with_scope(|ctx| {
            let body = self.check_bind(args, ctx)?;

            // Substitute the bound variables in the body or just body
            let mut checker = SubstituteChecker { ctx };
            Some(checker.ty(&body, pol).unwrap_or(body))
        })
    }

    pub fn check_bind(&self, args: &Interned<ArgsTy>, ctx: &mut impl TyCtxMut) -> Option<Ty> {
        let SigShape { sig, withs } = self.shape(ctx)?;

        // todo: check if the signature has free variables
        // let has_free_vars = sig.has_free_variables;

        for (arg_recv, arg_ins) in sig.matches(args, withs) {
            if let Ty::Var(arg_recv) = arg_recv {
                log::debug!("bind {arg_recv:?} {arg_ins:?}");
                ctx.bind_local(arg_recv, arg_ins.clone());
            }
        }

        sig.body.clone()
    }
}

struct SubstituteChecker<'a, T: TyCtxMut> {
    ctx: &'a mut T,
}

impl<'a, T: TyCtxMut> SubstituteChecker<'a, T> {
    fn ty(&mut self, body: &Ty, pol: bool) -> Option<Ty> {
        body.mutate(pol, self)
    }
}

impl<'a, T: TyCtxMut> TyMutator for SubstituteChecker<'a, T> {
    fn mutate(&mut self, ty: &Ty, pol: bool) -> Option<Ty> {
        // todo: extrude the type into a polarized type
        let _ = pol;

        if let Ty::Var(v) = ty {
            self.ctx.local_bind_of(v)
        } else {
            self.mutate_rec(ty, pol)
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::{assert_debug_snapshot, assert_snapshot};
    use tinymist_derive::BindTyCtx;

    use crate::ty::tests::*;

    use super::{ApplyChecker, Interned, Ty, TyCtx, TypeBounds, TypeScheme, TypeVar};
    #[test]
    fn test_ty() {
        use super::*;
        let ty = Ty::Builtin(BuiltinTy::Clause);
        let ty_ref = TyRef::new(ty.clone());
        assert_debug_snapshot!(ty_ref, @"Clause");
    }

    #[derive(Default, BindTyCtx)]
    #[bind(0)]
    struct CallCollector(TypeScheme, Vec<Ty>);

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
