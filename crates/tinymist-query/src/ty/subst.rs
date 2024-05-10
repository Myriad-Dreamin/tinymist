use hashbrown::HashMap;

use crate::{adt::interner::Interned, analysis::*, ty::def::*};

impl<'a> Sig<'a> {
    pub fn call(&self, args: &Interned<ArgsTy>, pol: bool) -> Option<Ty> {
        let (bound_variables, body) = self.check_bind(args)?;

        if bound_variables.is_empty() {
            return body;
        }

        let mut checker = SubstituteChecker { bound_variables };
        checker.ty(&body?, pol)
    }

    pub fn check_bind(&self, args: &Interned<ArgsTy>) -> Option<(HashMap<DefId, Ty>, Option<Ty>)> {
        let SigShape { sig, withs } = self.shape(None)?;

        let has_free_vars = sig.has_free_variables;

        let mut arguments = HashMap::new();
        for (arg_recv, arg_ins) in sig.matches(args, withs) {
            if has_free_vars {
                if let Ty::Var(arg_recv) = arg_recv {
                    arguments.insert(arg_recv.def, arg_ins.clone());
                }
            }
        }

        Some((arguments, sig.ret.clone()))
    }
}

// todo
struct SubstituteChecker {
    bound_variables: HashMap<DefId, Ty>,
}
impl SubstituteChecker {
    fn ty(&mut self, body: &Ty, pol: bool) -> Option<Ty> {
        let _ = self.bound_variables;
        let _ = pol;

        Some(body.clone())
    }
}
