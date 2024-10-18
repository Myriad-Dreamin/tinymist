//! Type checking at apply site

use super::*;
use crate::adt::interner::Interned;
use crate::ty::Sig;
use crate::{analysis::ApplyChecker, ty::ArgsTy};

#[derive(BindTyCtx)]
#[bind(base)]
pub struct ApplyTypeChecker<'a, 'b, 'w> {
    pub(super) base: &'a mut TypeChecker<'b, 'w>,
    pub call_site: Span,
    pub call_raw_for_with: Option<Ty>,
    pub resultant: Vec<Ty>,
}

impl<'a, 'b, 'w> ApplyChecker for ApplyTypeChecker<'a, 'b, 'w> {
    fn apply(&mut self, sig: Sig, args: &Interned<ArgsTy>, pol: bool) {
        let (sig, is_partialize) = match sig {
            Sig::Partialize(sig) => (*sig, true),
            sig => (sig, false),
        };

        if !is_partialize {
            if let Some(ty) = sig.call(args, pol, self.base) {
                self.resultant.push(ty);
            }
        }

        // todo: remove this after we implemented dependent types
        match sig {
            Sig::TypeCons { val, .. } => {
                if *val == typst::foundations::Type::of::<typst::foundations::Type>() {
                    if let Some(p0) = args.pos(0) {
                        self.resultant
                            .push(Ty::Unary(TypeUnary::new(UnaryOp::TypeOf, p0.clone())));
                    }
                }
            }
            Sig::Builtin(BuiltinSig::TupleMap(this)) => {
                if let Some(p0) = args.pos(0) {
                    let mut resultants = vec![];
                    log::debug!("syntax check tuple map {this:?} {p0:?}");
                    let mut mapper = |base: &mut TypeChecker, sig: Sig<'_>, _pol| {
                        resultants.push(Ty::Any);

                        match sig {
                            Sig::TupleCons(cons) => {
                                let res = cons
                                    .iter()
                                    .map(|elem| {
                                        log::debug!(
                                            "tuple map check on tuple elem: {elem:?} {p0:?}"
                                        );
                                        let args = ArgsTy::unary(elem.clone(), Ty::Any);
                                        let mut mapper = ApplyTypeChecker {
                                            base,
                                            call_site: Span::detached(),
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
                                log::debug!("array map check on array: {elem:?} {p0:?}");
                                let args = ArgsTy::unary(elem.as_ref().clone(), Ty::Any);
                                let mut mapper = ApplyTypeChecker {
                                    base,
                                    call_site: Span::detached(),
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
                    log::debug!("resultant: {resultants:?}");
                }
            }
            Sig::Builtin(BuiltinSig::TupleAt(this)) => {
                if let Some(p0) = args.pos(0) {
                    let mut resultants = vec![];
                    log::debug!("syntax check tuple at {this:?} {p0:?}");

                    // todo: caster
                    let selector = match p0 {
                        Ty::Value(v) => match v.val {
                            Value::Int(i) => Ok(i as usize),
                            Value::Float(i) => Ok(i as usize),
                            _ => Err(p0),
                        },
                        ty => Err(ty),
                    };

                    let mut mapper = |_base: &mut TypeChecker, sig: Sig<'_>, _pol| {
                        resultants.push(Ty::Any);

                        match sig {
                            Sig::TupleCons(cons) => {
                                log::debug!("tuple at check on tuple elem: {cons:?} {p0:?}");
                                let sel = match selector {
                                    Ok(i) => cons.get(i).cloned(),
                                    Err(_) => None,
                                };

                                let res =
                                    sel.unwrap_or_else(|| Ty::from_types(cons.iter().cloned()));
                                self.resultant.push(res);
                            }
                            Sig::ArrayCons(elem) => {
                                log::debug!("array at check on array: {elem:?} {p0:?}");
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
                    log::debug!("resultant: {resultants:?}");
                }
            }
            _ => {}
        }

        let callee = sig.ty();

        let Some(SigShape { sig, withs }) = sig.shape(self.base) else {
            return;
        };
        for (arg_recv, arg_ins) in sig.matches(args, withs) {
            self.base.constrain(arg_ins, arg_recv);
        }

        if let Some(callee) = callee.clone() {
            self.base.info.witness_at_least(self.call_site, callee);
        }

        if is_partialize {
            log::debug!("Partialize location {sig:?} a.k.a {callee:?}");
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
pub struct TupleChecker<'a, 'b, 'w> {
    pub(super) base: &'a mut TypeChecker<'b, 'w>,
    driver: &'a mut dyn TupleCheckDriver,
}

impl<'a, 'b, 'w> ApplyChecker for TupleChecker<'a, 'b, 'w> {
    fn apply(&mut self, sig: Sig, _args: &Interned<ArgsTy>, pol: bool) {
        self.driver.check(self.base, sig, pol);
    }
}
