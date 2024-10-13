use hashbrown::HashMap;

use crate::{adt::interner::Interned, analysis::*, ty::def::*};

impl<'a> Sig<'a> {
    pub fn call(
        &self,
        args: &Interned<ArgsTy>,
        pol: bool,
        ctx: Option<&mut AnalysisContext>,
    ) -> Option<Ty> {
        let (bound_variables, body) = self.check_bind(args, ctx)?;

        if bound_variables.is_empty() {
            return body;
        }

        let body = body?;

        // Substitute the bound variables in the body or just body
        let mut checker = SubstituteChecker { bound_variables };
        Some(checker.ty(&body, pol).unwrap_or(body))
    }

    pub fn check_bind(
        &self,
        args: &Interned<ArgsTy>,
        ctx: Option<&mut AnalysisContext>,
    ) -> Option<(HashMap<DefId, Ty>, Option<Ty>)> {
        let SigShape { sig, withs } = self.shape(ctx)?;

        // todo: check if the signature has free variables
        // let has_free_vars = sig.has_free_variables;
        let has_free_vars = true;

        let mut arguments = HashMap::new();
        if has_free_vars {
            for (arg_recv, arg_ins) in sig.matches(args, withs) {
                if let Ty::Var(arg_recv) = arg_recv {
                    arguments.insert(arg_recv.def, arg_ins.clone());
                }
            }
        }

        Some((arguments, sig.body.clone()))
    }
}

struct SubstituteChecker {
    bound_variables: HashMap<DefId, Ty>,
}

impl SubstituteChecker {
    fn ty(&mut self, body: &Ty, pol: bool) -> Option<Ty> {
        body.mutate(pol, self)
    }
}

impl MutateDriver for SubstituteChecker {
    fn mutate(&mut self, ty: &Ty, pol: bool) -> Option<Ty> {
        // todo: extrude the type into a polarized type
        let _ = pol;

        Some(match ty {
            // todo: substitute the bound in the type
            Ty::Let(..) => return None,
            Ty::Var(v) => {
                if let Some(ty) = self.bound_variables.get(&v.def) {
                    ty.clone()
                } else {
                    return None;
                }
            }
            Ty::Value(..) | Ty::Any | Ty::Boolean(..) | Ty::Builtin(..) => return None,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use insta::{assert_debug_snapshot, assert_snapshot};

    use crate::ty::tests::*;

    use super::{ApplyChecker, Ty, TyCtx};
    #[test]
    fn test_ty() {
        use super::*;
        let ty = Ty::Builtin(BuiltinTy::Clause);
        let ty_ref = TyRef::new(ty.clone());
        assert_debug_snapshot!(ty_ref, @"Clause");
    }

    #[derive(Default)]
    struct CallCollector(Vec<Ty>);

    impl TyCtx for CallCollector {}
    impl ApplyChecker for CallCollector {
        fn apply(
            &mut self,
            sig: super::Sig,
            arguments: &crate::adt::interner::Interned<super::ArgsTy>,
            pol: bool,
        ) {
            let ty = sig.call(arguments, pol, None);
            if let Some(ty) = ty {
                self.0.push(ty);
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

            collector.0.iter().fold(String::new(), |mut acc, ty| {
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
