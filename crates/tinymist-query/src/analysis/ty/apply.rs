//! Type checking at apply site

use typst::syntax::{ast, Span};

use crate::analysis::Ty;
use crate::ty::Sig;
use crate::{analysis::ApplyChecker, ty::ArgsTy};

use super::*;
use crate::adt::interner::Interned;

pub struct ApplyTypeChecker<'a, 'b, 'w> {
    pub(super) base: &'a mut TypeChecker<'b, 'w>,
    pub call_site: Span,
    pub args: ast::Args<'a>,
    pub resultant: Vec<Ty>,
}

impl<'a, 'b, 'w> ApplyChecker for ApplyTypeChecker<'a, 'b, 'w> {
    fn bound_of_var(
        &mut self,
        var: &Interned<super::TypeVar>,
        _pol: bool,
    ) -> Option<super::TypeBounds> {
        self.base
            .info
            .vars
            .get(&var.def)
            .map(|v| v.bounds.bounds().read().clone())
    }

    fn apply(&mut self, sig: Sig, args: &Interned<ArgsTy>, pol: bool) {
        let _ = self.args;

        let (sig, is_partialize) = match sig {
            Sig::Partialize(sig) => (*sig, true),
            sig => (sig, false),
        };

        if !is_partialize {
            if let Some(ty) = sig.call(args, pol, Some(self.base.ctx)) {
                self.resultant.push(ty);
            }
        }

        // todo: remove this after we implemented dependent types
        if let Sig::TypeCons { val, .. } = sig {
            if *val == typst::foundations::Type::of::<typst::foundations::Type>() {
                if let Some(p0) = args.pos(0) {
                    self.resultant
                        .push(Ty::Unary(TypeUnary::new(UnaryOp::TypeOf, p0.into())));
                }
            }
        }
        // let v = val.inner();
        // use typst::foundations::func::Repr;
        // if let Repr::Native(v) = v {
        //     match v.name {
        //         "assert" => {}
        //         "panic" => {}
        //         _ => {}
        //     }
        // }

        let callee = sig.ty();

        let Some(SigShape { sig, withs }) = sig.shape(Some(self.base.ctx)) else {
            return;
        };
        for (arg_recv, arg_ins) in sig.matches(args, withs) {
            self.base.constrain(arg_ins, arg_recv);
        }

        if let Some(callee) = callee.clone() {
            self.base.info.witness_at_least(self.call_site, callee);
        }

        if is_partialize {
            let Some(sig) = callee else {
                log::warn!("Partialize is not implemented yet {sig:?}");
                return;
            };
            self.resultant
                .push(Ty::With(SigWithTy::new(sig.into(), args.clone())));
        }

        //            let f = v.as_ref();
        //            let mut pos = f.pos.iter();
        //            // let mut named = f.named.clone();
        //            // let mut rest = f.rest.clone();

        //            for pos_in in args.start_match() {
        //                let pos_ty = pos.next().unwrap_or(&FlowType::Any);
        //                self.constrain(pos_in, pos_ty);
        //            }

        //            for (name, named_in) in &args.named {
        //                let named_ty = f.named.iter().find(|(n, _)| n ==
        //    name).map(|(_, ty)| ty);             if let Some(named_ty) =
        //    named_ty {                 self.constrain(named_in,
        //    named_ty);             }
        //            }'

        //    todo: hold signature
        //     self.info.witness_at_least(
        //         callee_span,
        //         FlowType::Value(TypeIns::new(Value::Func(f.clone()))),
        //     );
    }
}
