use typst::foundations::{Func, Value};

use crate::{adt::interner::Interned, analysis::*, ty::def::*};

pub enum Sig<'a> {
    Type(&'a Interned<SigTy>),
    ArrayCons(&'a TyRef),
    DictCons(&'a Interned<RecordTy>),
    Value(&'a Func),
    With {
        sig: &'a Sig<'a>,
        withs: &'a Vec<Interned<ArgsTy>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigSurfaceKind {
    Call,
    Array,
    Dict,
    ArrayOrDict,
}

pub trait SigChecker {
    fn check(&mut self, sig: Sig, args: &mut SigCheckContext, pol: bool) -> Option<()>;
    fn check_var(&mut self, _var: &Interned<TypeVar>, _pol: bool) -> Option<TypeBounds> {
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
        let context = SigCheckContext {
            sig_kind,
            args: Vec::new(),
        };
        let mut worker = SigCheckDriver {
            ctx: context,
            checker,
        };

        worker.ty(self, pol);
    }
}

pub struct SigCheckContext {
    pub sig_kind: SigSurfaceKind,
    pub args: Vec<Interned<SigTy>>,
}

pub struct SigCheckDriver<'a> {
    ctx: SigCheckContext,
    checker: &'a mut dyn SigChecker,
}

impl<'a> SigCheckDriver<'a> {
    fn func_as_sig(&self) -> bool {
        matches!(self.ctx.sig_kind, SigSurfaceKind::Call)
    }

    fn array_as_sig(&self) -> bool {
        matches!(
            self.ctx.sig_kind,
            SigSurfaceKind::Array | SigSurfaceKind::ArrayOrDict
        )
    }

    fn dict_as_sig(&self) -> bool {
        matches!(
            self.ctx.sig_kind,
            SigSurfaceKind::Dict | SigSurfaceKind::ArrayOrDict
        )
    }

    fn ty(&mut self, ty: &Ty, pol: bool) {
        match ty {
            Ty::Builtin(BuiltinTy::Stroke) if self.dict_as_sig() => {
                self.checker
                    .check(Sig::DictCons(&FLOW_STROKE_DICT), &mut self.ctx, pol);
            }
            Ty::Builtin(BuiltinTy::Margin) if self.dict_as_sig() => {
                self.checker
                    .check(Sig::DictCons(&FLOW_MARGIN_DICT), &mut self.ctx, pol);
            }
            Ty::Builtin(BuiltinTy::Inset) if self.dict_as_sig() => {
                self.checker
                    .check(Sig::DictCons(&FLOW_INSET_DICT), &mut self.ctx, pol);
            }
            Ty::Builtin(BuiltinTy::Outset) if self.dict_as_sig() => {
                self.checker
                    .check(Sig::DictCons(&FLOW_OUTSET_DICT), &mut self.ctx, pol);
            }
            Ty::Builtin(BuiltinTy::Radius) if self.dict_as_sig() => {
                self.checker
                    .check(Sig::DictCons(&FLOW_RADIUS_DICT), &mut self.ctx, pol);
            }
            // todo: deduplicate checking early
            Ty::Value(v) => {
                if self.func_as_sig() {
                    if let Value::Func(f) = &v.val {
                        self.checker.check(Sig::Value(f), &mut self.ctx, pol);
                    }
                }
            }
            Ty::Builtin(BuiltinTy::Element(e)) if self.func_as_sig() => {
                // todo: distinguish between element and function
                let f = (*e).into();
                self.checker.check(Sig::Value(&f), &mut self.ctx, pol);
            }
            Ty::Func(sig) if self.func_as_sig() => {
                self.checker.check(Sig::Type(sig), &mut self.ctx, pol);
            }
            Ty::Array(sig) if self.array_as_sig() => {
                // let sig = FlowSignature::array_cons(*sig.clone(), true);
                self.checker.check(Sig::ArrayCons(sig), &mut self.ctx, pol);
            }
            // todo: tuple
            Ty::Tuple(_) => {}
            Ty::Dict(sig) if self.dict_as_sig() => {
                // self.check_dict_signature(sig, pol, self.checker);
                self.checker.check(Sig::DictCons(sig), &mut self.ctx, pol);
            }
            Ty::With(w) if self.func_as_sig() => {
                self.ctx.args.push(w.with.clone());
                self.ty(&w.sig, pol);
                self.ctx.args.pop();
            }
            Ty::Select(sel) => sel.ty.bounds(pol, &mut MethodDriver(self, &sel.select)),
            // todo: calculate these operators
            Ty::Unary(_) => {}
            Ty::Binary(_) => {}
            Ty::If(_) => {}
            _ if ty.has_bounds() => ty.bounds(pol, self),
            _ => {}
        }
    }
}

impl BoundChecker for SigCheckDriver<'_> {
    fn collect(&mut self, ty: &Ty, pol: bool) {
        self.ty(ty, pol);
    }

    fn bound_of_var(&mut self, var: &Interned<TypeVar>, pol: bool) -> Option<TypeBounds> {
        self.checker.check_var(var, pol)
    }
}

struct MethodDriver<'a, 'b>(&'a mut SigCheckDriver<'b>, &'a Interned<str>);

impl<'a, 'b> MethodDriver<'a, 'b> {
    fn is_binder(&self) -> bool {
        matches!(self.1.as_ref(), "with" | "where")
    }
}

impl<'a, 'b> BoundChecker for MethodDriver<'a, 'b> {
    fn collect(&mut self, ty: &Ty, pol: bool) {
        match ty {
            // todo: deduplicate checking early
            Ty::Value(v) => {
                if let Value::Func(f) = &v.val {
                    if self.is_binder() {
                        self.0.checker.check(Sig::Value(f), &mut self.0.ctx, pol);
                    } else {
                        // todo: general select operator
                    }
                }
            }
            Ty::Builtin(BuiltinTy::Element(e)) => {
                // todo: distinguish between element and function
                if self.is_binder() {
                    let f = (*e).into();
                    self.0.checker.check(Sig::Value(&f), &mut self.0.ctx, pol);
                } else {
                    // todo: general select operator
                }
            }
            // todo: general select operator
            _ => {}
        }
    }

    fn bound_of_var(&mut self, var: &Interned<TypeVar>, pol: bool) -> Option<TypeBounds> {
        self.0.checker.check_var(var, pol)
    }
}
