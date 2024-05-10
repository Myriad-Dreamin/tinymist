use typst::foundations::Value;

use crate::{adt::interner::Interned, analysis::*, ty::def::*};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigSurfaceKind {
    Call,
    Array,
    Dict,
    ArrayOrDict,
}

pub trait SigChecker {
    fn check(&mut self, sig: Sig, args: &mut SigCheckContext, pol: bool) -> Option<()>;
    fn check_var(&mut self, _var: Interned<TypeVar>, _pol: bool) -> Option<TypeBounds> {
        None
    }
}

impl<T> SigChecker for T
where
    T: FnMut(Sig, &mut SigCheckContext, bool) -> Option<()>,
{
    fn check(&mut self, sig: Sig, args: &mut SigCheckContext, pol: bool) -> Option<()> {
        self(sig, args, pol)
    }
}

impl Ty {
    pub fn sig_surface(&self, pol: bool, sig_kind: SigSurfaceKind, checker: &mut impl SigChecker) {
        let mut worker = SigCheckContext {
            sig_kind,
            args: Vec::new(),
        };

        worker.ty(self, pol, checker);
    }
}

pub struct SigCheckContext {
    sig_kind: SigSurfaceKind,
    args: Vec<Interned<SigTy>>,
}

impl SigCheckContext {
    fn func_as_sig(&self) -> bool {
        matches!(self.sig_kind, SigSurfaceKind::Call)
    }

    fn array_as_sig(&self) -> bool {
        matches!(
            self.sig_kind,
            SigSurfaceKind::Array | SigSurfaceKind::ArrayOrDict
        )
    }

    fn dict_as_sig(&self) -> bool {
        matches!(
            self.sig_kind,
            SigSurfaceKind::Dict | SigSurfaceKind::ArrayOrDict
        )
    }

    fn tys<'a>(
        &mut self,
        tys: impl Iterator<Item = &'a Ty>,
        pol: bool,
        checker: &mut impl SigChecker,
    ) {
        for ty in tys {
            self.ty(ty, pol, checker);
        }
    }

    fn ty(&mut self, ty: &Ty, pol: bool, checker: &mut impl SigChecker) {
        match ty {
            Ty::Builtin(BuiltinTy::Stroke) if self.dict_as_sig() => {
                checker.check(Sig::DictCons(&FLOW_STROKE_DICT), self, pol);
            }
            Ty::Builtin(BuiltinTy::Margin) if self.dict_as_sig() => {
                checker.check(Sig::DictCons(&FLOW_MARGIN_DICT), self, pol);
            }
            Ty::Builtin(BuiltinTy::Inset) if self.dict_as_sig() => {
                checker.check(Sig::DictCons(&FLOW_INSET_DICT), self, pol);
            }
            Ty::Builtin(BuiltinTy::Outset) if self.dict_as_sig() => {
                checker.check(Sig::DictCons(&FLOW_OUTSET_DICT), self, pol);
            }
            Ty::Builtin(BuiltinTy::Radius) if self.dict_as_sig() => {
                checker.check(Sig::DictCons(&FLOW_RADIUS_DICT), self, pol);
            }
            // todo: deduplicate checking early
            Ty::Value(v) => {
                if self.func_as_sig() {
                    if let Value::Func(f) = &v.val {
                        checker.check(Sig::Value(f), self, pol);
                    }
                }
            }
            Ty::Builtin(BuiltinTy::Element(e)) if self.func_as_sig() => {
                // todo: distinguish between element and function
                let f = (*e).into();
                checker.check(Sig::Value(&f), self, pol);
            }
            Ty::Func(sig) if self.func_as_sig() => {
                checker.check(Sig::Type(sig), self, pol);
            }
            Ty::Array(sig) if self.array_as_sig() => {
                // let sig = FlowSignature::array_cons(*sig.clone(), true);
                checker.check(Sig::ArrayCons(sig), self, pol);
            }
            // todo: tuple
            Ty::Tuple(_) => {}
            Ty::Dict(sig) if self.dict_as_sig() => {
                // self.check_dict_signature(sig, pol, checker);
                checker.check(Sig::DictCons(sig), self, pol);
            }
            Ty::With(w) if self.func_as_sig() => {
                self.args.push(w.with.clone());
                self.ty(&w.sig, pol, checker);
                self.args.pop();
            }
            Ty::Union(u) => {
                self.tys(u.iter(), pol, checker);
            }
            Ty::Let(u) => {
                self.tys(u.ubs.iter(), pol, checker);
                self.tys(u.lbs.iter(), !pol, checker);
            }
            Ty::Var(u) => {
                let Some(w) = checker.check_var(u.clone(), pol) else {
                    return;
                };
                self.tys(w.ubs.iter(), pol, checker);
                self.tys(w.lbs.iter(), !pol, checker);
            }
            // todo: calculate these operators
            Ty::Select(_) => {
            }
            // todo: calculate these operators
            Ty::Unary(_) => {}
            Ty::Binary(_) => {}
            Ty::If(_) => {}
            _ => {}
        }
    }
}
