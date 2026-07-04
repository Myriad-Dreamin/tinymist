//! Type checking at apply site

use super::*;
use crate::adt::interner::Interned;
use crate::ty::Sig;
use crate::{analysis::ApplyChecker, ty::ArgsTy};

#[derive(BindTyCtx)]
#[bind(base)]
pub struct ApplyTypeChecker<'a, 'b> {
    pub(super) base: &'a mut TypeChecker<'b>,
    pub call_site: Span,
    pub allow_deferred_calls: bool,
    pub call_raw_for_with: Option<Ty>,
    pub resultant: Vec<Ty>,
}

impl ApplyChecker for ApplyTypeChecker<'_, '_> {
    fn apply(&mut self, sig: Sig, args: &Interned<ArgsTy>, pol: bool) {
        let (sig, is_partialize) = match sig {
            Sig::Partialize(sig) => (*sig, true),
            sig => (sig, false),
        };

        let shape = sig.shape(self.base);

        if !is_partialize
            && let Some(SigShape {
                sig: shape_sig,
                withs,
            }) = &shape
        {
            let args_have_generated = TypeChecker::sig_has_generated_var(args)
                || withs
                    .map(|withs| {
                        withs
                            .iter()
                            .any(|with| TypeChecker::sig_has_generated_var(with))
                    })
                    .unwrap_or(false);
            let can_defer = shape_sig
                .body
                .as_ref()
                .is_some_and(TypeChecker::has_generated_var)
                && self.allow_deferred_calls
                && !args_have_generated
                && !shape_sig
                    .body
                    .as_ref()
                    .is_some_and(|body| self.base.is_inferring_return(body));

            if can_defer {
                if let Some(ty) = self.base.instantiate_sig_result(shape_sig, args, *withs)
                    && !TypeChecker::has_generated_var(&ty)
                {
                    self.resultant.push(ty);
                } else {
                    let ret = self.base.fresh_generated_var(Interned::empty().clone());
                    self.base.defer_call(
                        shape_sig.clone(),
                        args.clone(),
                        withs.cloned(),
                        ret.clone(),
                    );
                    self.resultant.push(ret);
                }
            } else if let Some(ty) = sig.call(args, pol, self.base) {
                self.resultant.push(ty);
            }
        }

        // todo: remove this after we implemented dependent types
        match sig {
            Sig::TypeCons { val, .. } => {
                if *val == typst::foundations::Type::of::<typst::foundations::Type>()
                    && let Some(p0) = args.pos(0)
                {
                    self.resultant.push(p0.type_of_result());
                }
            }
            Sig::Builtin(BuiltinSig::TupleMap(this)) => {
                if let Some(p0) = args.pos(0) {
                    let mut resultants = vec![];
                    crate::log_debug_ct!("syntax check tuple map {this:?} {p0:?}");
                    let mut mapper = |base: &mut TypeChecker, sig: Sig<'_>, _pol| {
                        resultants.push(Ty::Any);

                        match sig {
                            Sig::TupleCons(cons) => {
                                let res = cons
                                    .iter()
                                    .map(|elem| {
                                        crate::log_debug_ct!(
                                            "tuple map check on tuple elem: {elem:?} {p0:?}"
                                        );
                                        let args = ArgsTy::unary(elem.clone(), Ty::Any);
                                        let mut mapper = ApplyTypeChecker {
                                            base,
                                            call_site: Span::detached(),
                                            allow_deferred_calls: false,
                                            call_raw_for_with: None,
                                            resultant: vec![],
                                        };
                                        p0.call(&args, true, &mut mapper);
                                        Ty::from_types(mapper.resultant.into_iter())
                                    })
                                    .collect::<Vec<_>>();
                                self.resultant.push(Ty::Tuple(res.into()));
                            }
                            Sig::ArrayCons(elem) => {
                                crate::log_debug_ct!("array map check on array: {elem:?} {p0:?}");
                                let args = ArgsTy::unary(elem.as_ref().clone(), Ty::Any);
                                let mut mapper = ApplyTypeChecker {
                                    base,
                                    call_site: Span::detached(),
                                    allow_deferred_calls: false,
                                    call_raw_for_with: None,
                                    resultant: vec![],
                                };
                                p0.call(&args, true, &mut mapper);
                                let res = Ty::from_types(mapper.resultant.into_iter());
                                self.resultant.push(Ty::Array(res.into()));
                            }
                            _ => {}
                        }
                    };
                    let mut worker = TupleChecker {
                        base: self.base,
                        driver: &mut mapper,
                    };
                    this.tuple_element_of(pol, &mut worker);
                    crate::log_debug_ct!("resultant: {resultants:?}");
                }
            }
            Sig::Builtin(BuiltinSig::TupleAt(this)) => {
                if let Some(p0) = args.pos(0) {
                    let mut resultants = vec![];
                    crate::log_debug_ct!("syntax check tuple at {this:?} {p0:?}");

                    // todo: caster
                    let arg_offset = match p0 {
                        Ty::Value(v) => match v.val {
                            Value::Int(arg_offset) => Ok(arg_offset as usize),
                            Value::Float(arg_offset) => Ok(arg_offset as usize),
                            _ => Err(p0),
                        },
                        ty => Err(ty),
                    };

                    let mut mapper = |_base: &mut TypeChecker, sig: Sig<'_>, _pol| {
                        resultants.push(Ty::Any);

                        match sig {
                            Sig::TupleCons(cons) => {
                                crate::log_debug_ct!(
                                    "tuple at check on tuple elem: {cons:?} {p0:?}"
                                );
                                let sel = match arg_offset {
                                    Ok(arg_offset) => cons.get(arg_offset).cloned(),
                                    Err(_) => None,
                                };

                                let res =
                                    sel.unwrap_or_else(|| Ty::from_types(cons.iter().cloned()));
                                self.resultant.push(res);
                            }
                            Sig::ArrayCons(elem) => {
                                crate::log_debug_ct!("tuple at check on array: {elem:?} {p0:?}");
                                self.resultant.push(elem.as_ref().clone());
                            }
                            _ => {}
                        }
                    };
                    let mut worker = TupleChecker {
                        base: self.base,
                        driver: &mut mapper,
                    };
                    this.tuple_element_of(pol, &mut worker);
                    crate::log_debug_ct!("resultant: {resultants:?}");
                }
            }
            _ => {}
        }

        let callee = sig.ty();

        let Some(SigShape { sig, withs }) = shape else {
            return;
        };
        self.base.constrain_sig_inputs(&sig, args, withs);

        if let Some(callee) = callee.clone() {
            self.base.witness_at_least(self.call_site, callee);
        }

        if is_partialize {
            crate::log_debug_ct!("Partialize location {sig:?} a.k.a {callee:?}");
            if let Some(Ty::Select(call_raw_for_with)) = self.call_raw_for_with.take() {
                self.resultant.push(Ty::With(SigWithTy::new(
                    call_raw_for_with.ty.clone(),
                    args.clone(),
                )));
            }
        }
    }
}

trait TupleCheckDriver {
    fn check(&mut self, base: &mut TypeChecker, sig: Sig, pol: bool);
}

impl<T: FnMut(&mut TypeChecker, Sig, bool)> TupleCheckDriver for T {
    fn check(&mut self, base: &mut TypeChecker, sig: Sig, pol: bool) {
        self(base, sig, pol);
    }
}

#[derive(BindTyCtx)]
#[bind(base)]
pub struct TupleChecker<'a, 'b> {
    pub(super) base: &'a mut TypeChecker<'b>,
    driver: &'a mut dyn TupleCheckDriver,
}

impl ApplyChecker for TupleChecker<'_, '_> {
    fn apply(&mut self, sig: Sig, _args: &Interned<ArgsTy>, pol: bool) {
        self.driver.check(self.base, sig, pol);
    }
}
