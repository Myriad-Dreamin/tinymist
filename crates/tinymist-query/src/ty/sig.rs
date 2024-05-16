use typst::foundations::{Func, Value};

use crate::{adt::interner::Interned, analysis::*, ty::def::*};

#[derive(Debug, Clone, Copy)]
pub enum Sig<'a> {
    Type(&'a Interned<SigTy>),
    TypeCons {
        val: &'a typst::foundations::Type,
        at: &'a Ty,
    },
    ArrayCons(&'a TyRef),
    DictCons(&'a Interned<RecordTy>),
    Value {
        val: &'a Func,
        at: &'a Ty,
    },
    Partialize(&'a Sig<'a>),
    With {
        sig: &'a Sig<'a>,
        withs: &'a Vec<Interned<ArgsTy>>,
        at: &'a Ty,
    },
}

pub struct SigShape<'a> {
    pub sig: Interned<SigTy>,
    pub withs: Option<&'a Vec<Interned<SigTy>>>,
}

impl<'a> Sig<'a> {
    pub fn ty(self) -> Option<Ty> {
        Some(match self {
            Sig::Type(t) => Ty::Func(t.clone()),
            Sig::ArrayCons(t) => Ty::Array(t.clone()),
            Sig::DictCons(t) => Ty::Dict(t.clone()),
            Sig::TypeCons { val, .. } => Ty::Builtin(BuiltinTy::Type(*val)),
            Sig::Value { at, .. } => at.clone(),
            Sig::With { at, .. } => at.clone(),
            Sig::Partialize(..) => return None,
        })
    }

    pub fn shape(self, ctx: Option<&mut AnalysisContext>) -> Option<SigShape<'a>> {
        let (cano_sig, withs) = match self {
            Sig::With { sig, withs, .. } => (*sig, Some(withs)),
            _ => (self, None),
        };

        let sig_ins = match cano_sig {
            Sig::ArrayCons(a) => SigTy::array_cons(a.as_ref().clone(), false),
            Sig::DictCons(d) => SigTy::dict_cons(d, false),
            Sig::TypeCons { val, .. } => ctx?.type_of_func(&val.constructor().ok()?)?,
            Sig::Value { val, .. } => ctx?.type_of_func(val)?,
            // todo
            Sig::Partialize(..) => return None,
            Sig::With { .. } => return None,
            Sig::Type(t) => t.clone(),
        };

        Some(SigShape {
            sig: sig_ins,
            withs,
        })
    }
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
    /// Iterate over the signatures of the given type.
    pub fn sig_surface(&self, pol: bool, sig_kind: SigSurfaceKind, checker: &mut impl SigChecker) {
        let context = SigCheckContext {
            sig_kind,
            args: Vec::new(),
            at: TyRef::new(Ty::Any),
        };
        let mut worker = SigCheckDriver {
            ctx: context,
            checker,
        };

        worker.ty(self, pol);
    }

    /// Get the signature representation of the given type.
    pub fn sig_repr(&self, pol: bool) -> Option<Interned<SigTy>> {
        // todo: union sig
        // let mut pos = vec![];
        // let mut named = HashMap::new();
        // let mut rest = None;
        // let mut ret = None;

        let mut primary = None;

        self.sig_surface(
            pol,
            SigSurfaceKind::Call,
            &mut |sig: Sig, _ctx: &mut SigCheckContext, _pol: bool| {
                let sig = sig.shape(None)?;
                primary = Some(sig.sig.clone());
                Some(())
            },
        );

        primary
    }
}

pub struct SigCheckContext {
    pub sig_kind: SigSurfaceKind,
    pub args: Vec<Interned<SigTy>>,
    pub at: TyRef,
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
                    match &v.val {
                        Value::Func(f) => {
                            self.checker
                                .check(Sig::Value { val: f, at: ty }, &mut self.ctx, pol);
                        }
                        Value::Type(t) => {
                            self.checker.check(
                                Sig::TypeCons { val: t, at: ty },
                                &mut self.ctx,
                                pol,
                            );
                        }
                        _ => {}
                    }
                }
            }
            Ty::Builtin(BuiltinTy::Type(e)) if self.func_as_sig() => {
                // todo: distinguish between element and function
                self.checker
                    .check(Sig::TypeCons { val: e, at: ty }, &mut self.ctx, pol);
            }
            Ty::Builtin(BuiltinTy::Element(e)) if self.func_as_sig() => {
                // todo: distinguish between element and function
                let f = (*e).into();
                self.checker
                    .check(Sig::Value { val: &f, at: ty }, &mut self.ctx, pol);
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

struct MethodDriver<'a, 'b>(&'a mut SigCheckDriver<'b>, &'a StrRef);

impl<'a, 'b> MethodDriver<'a, 'b> {
    fn is_binder(&self) -> bool {
        matches!(self.1.as_ref(), "with" | "where")
    }
}

impl<'a, 'b> BoundChecker for MethodDriver<'a, 'b> {
    fn collect(&mut self, ty: &Ty, pol: bool) {
        log::debug!("check method: {ty:?}.{}", self.1.as_ref());
        match ty {
            // todo: deduplicate checking early
            Ty::Value(v) => {
                if let Value::Func(f) = &v.val {
                    if self.is_binder() {
                        self.0.checker.check(
                            Sig::Partialize(&Sig::Value { val: f, at: ty }),
                            &mut self.0.ctx,
                            pol,
                        );
                    } else {
                        // todo: general select operator
                    }
                }
            }
            Ty::Builtin(BuiltinTy::Element(e)) => {
                // todo: distinguish between element and function
                if self.is_binder() {
                    let f = (*e).into();
                    self.0.checker.check(
                        Sig::Partialize(&Sig::Value { val: &f, at: ty }),
                        &mut self.0.ctx,
                        pol,
                    );
                } else {
                    // todo: general select operator
                }
            }
            Ty::Func(sig) => {
                if self.is_binder() {
                    self.0
                        .checker
                        .check(Sig::Partialize(&Sig::Type(sig)), &mut self.0.ctx, pol);
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
