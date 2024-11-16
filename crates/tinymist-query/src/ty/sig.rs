use typst::foundations::{Func, Value};

use super::BoundChecker;
use crate::ty::prelude::*;

#[derive(Debug, Clone, Copy)]
pub enum Sig<'a> {
    Builtin(BuiltinSig<'a>),
    Type(&'a Interned<SigTy>),
    TypeCons {
        val: &'a typst::foundations::Type,
        at: &'a Ty,
    },
    ArrayCons(&'a TyRef),
    TupleCons(&'a Interned<Vec<Ty>>),
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
            Sig::Builtin(_) => return None,
            Sig::Type(t) => Ty::Func(t.clone()),
            Sig::ArrayCons(t) => Ty::Array(t.clone()),
            Sig::TupleCons(t) => Ty::Tuple(t.clone()),
            Sig::DictCons(t) => Ty::Dict(t.clone()),
            Sig::TypeCons { val, .. } => Ty::Builtin(BuiltinTy::Type(*val)),
            Sig::Value { at, .. } => at.clone(),
            Sig::With { at, .. } => at.clone(),
            Sig::Partialize(..) => return None,
        })
    }

    pub fn shape(self, ctx: &mut impl TyCtxMut) -> Option<SigShape<'a>> {
        let (cano_sig, withs) = match self {
            Sig::With { sig, withs, .. } => (*sig, Some(withs)),
            _ => (self, None),
        };

        let sig_ins = match cano_sig {
            Sig::Builtin(_) => return None,
            Sig::ArrayCons(a) => SigTy::array_cons(a.as_ref().clone(), false),
            Sig::TupleCons(t) => SigTy::tuple_cons(t.clone(), false),
            Sig::DictCons(d) => SigTy::dict_cons(d, false),
            Sig::TypeCons { val, .. } => ctx.type_of_func(&val.constructor().ok()?)?,
            Sig::Value { val, .. } => ctx.type_of_func(val)?,
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

pub trait SigChecker: TyCtx {
    fn check(&mut self, sig: Sig, args: &mut SigCheckContext, pol: bool) -> Option<()>;
}

impl Ty {
    /// Iterate over the signatures of the given type.
    pub fn sig_surface(&self, pol: bool, sig_kind: SigSurfaceKind, checker: &mut impl SigChecker) {
        let ctx = SigCheckContext {
            sig_kind,
            args: Vec::new(),
            at: TyRef::new(Ty::Any),
        };

        SigCheckDriver { ctx, checker }.ty(self, pol);
    }

    /// Get the signature representation of the given type.
    pub fn sig_repr(&self, pol: bool, ctx: &mut impl TyCtxMut) -> Option<Interned<SigTy>> {
        // todo: union sig
        // let mut pos = vec![];
        // let mut named = HashMap::new();
        // let mut rest = None;
        // let mut ret = None;

        let mut primary = None;

        #[derive(BindTyCtx)]
        #[bind(0)]
        struct SigReprDriver<'a, C: TyCtxMut>(&'a mut C, &'a mut Option<Interned<SigTy>>);

        impl<C: TyCtxMut> SigChecker for SigReprDriver<'_, C> {
            fn check(&mut self, sig: Sig, _ctx: &mut SigCheckContext, _pol: bool) -> Option<()> {
                let sig = sig.shape(self.0)?;
                *self.1 = Some(sig.sig.clone());
                Some(())
            }
        }

        self.sig_surface(
            pol,
            SigSurfaceKind::Call,
            // todo: bind type context
            &mut SigReprDriver(ctx, &mut primary),
        );

        primary
    }
}

pub struct SigCheckContext {
    pub sig_kind: SigSurfaceKind,
    pub args: Vec<Interned<SigTy>>,
    pub at: TyRef,
}

#[derive(BindTyCtx)]
#[bind(checker)]
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
        log::debug!("check sig: {ty:?}");
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
                self.checker.check(Sig::ArrayCons(sig), &mut self.ctx, pol);
            }
            Ty::Tuple(tup) if self.array_as_sig() => {
                self.checker.check(Sig::TupleCons(tup), &mut self.ctx, pol);
            }
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
            Ty::Param(p) => {
                // todo: keep type information
                self.ty(&p.ty, pol);
            }
            _ if ty.has_bounds() => ty.bounds(pol, self),
            _ => {}
        }
    }
}

impl BoundChecker for SigCheckDriver<'_> {
    fn collect(&mut self, ty: &Ty, pol: bool) {
        log::debug!("sig bounds: {ty:?}");
        self.ty(ty, pol);
    }
}

#[derive(BindTyCtx)]
#[bind(0)]
struct MethodDriver<'a, 'b>(&'a mut SigCheckDriver<'b>, &'a StrRef);

impl<'a, 'b> MethodDriver<'a, 'b> {
    fn is_binder(&self) -> bool {
        matches!(self.1.as_ref(), "with" | "where")
    }

    fn array_method(&mut self, ty: &Ty, pol: bool) {
        let method = match self.1.as_ref() {
            "map" => BuiltinSig::TupleMap(ty),
            "at" => BuiltinSig::TupleAt(ty),
            _ => return,
        };
        self.0
            .checker
            .check(Sig::Builtin(method), &mut self.0.ctx, pol);
    }
}

impl<'a, 'b> BoundChecker for MethodDriver<'a, 'b> {
    fn collect(&mut self, ty: &Ty, pol: bool) {
        log::debug!("check method: {ty:?}.{}", self.1.as_ref());
        match ty {
            // todo: deduplicate checking early
            Ty::Value(v) => {
                match &v.val {
                    Value::Func(f) => {
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
                    Value::Array(..) => self.array_method(ty, pol),
                    _ => {}
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
            Ty::With(w) => {
                self.0.ctx.args.push(w.with.clone());
                w.sig.bounds(pol, self);
                self.0.ctx.args.pop();
            }
            Ty::Tuple(..) => self.array_method(ty, pol),
            Ty::Array(..) => self.array_method(ty, pol),
            // todo: general select operator
            _ => {}
        }
    }
}
