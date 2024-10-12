use typst::foundations::{Dict, Value};

use crate::{adt::interner::Interned, analysis::*, ty::def::*};

#[derive(Debug, Clone, Copy)]
pub enum Iface<'a> {
    Dict(&'a Interned<RecordTy>),
    Element {
        val: &'a typst::foundations::Element,
        at: &'a Ty,
    },
    Type {
        val: &'a typst::foundations::Type,
        at: &'a Ty,
    },
    Value {
        val: &'a Dict,
        at: &'a Ty,
    },
    ArrayCons(&'a TyRef),
    Partialize(&'a Iface<'a>),
}

pub struct IfaceShape {
    pub iface: Interned<RecordTy>,
}

impl<'a> Iface<'a> {
    pub fn ty(self) -> Option<Ty> {
        Some(match self {
            Iface::ArrayCons(t) => Ty::Array(t.clone()),
            Iface::Dict(t) => Ty::Dict(t.clone()),
            Iface::Type { val, .. } => Ty::Builtin(BuiltinTy::Type(*val)),
            Iface::Element { val, .. } => Ty::Builtin(BuiltinTy::Element(*val)),
            Iface::Value { at, .. } => at.clone(),
            Iface::Partialize(..) => return None,
        })
    }

    pub fn shape(self, _ctx: Option<&mut AnalysisContext>) -> Option<IfaceShape> {
        println!("iface shape: {self:?}");

        let record_ins = match self {
            // Iface::ArrayCons(a) => SigTy::array_cons(a.as_ref().clone(), false),
            Iface::ArrayCons(..) => return None,
            Iface::Dict(d) => d.clone(),
            // Iface::Type { val, .. } => ctx?.type_of_func(&val.constructor().ok()?)?,
            // Iface::Value { val, .. } => ctx?.type_of_func(val)?, // todo
            Iface::Partialize(..) => return None,
            Iface::Element { .. } => return None,
            Iface::Type { .. } => return None,
            Iface::Value { .. } => return None,
        };

        Some(IfaceShape { iface: record_ins })
    }
}

pub trait IfaceChecker {
    fn check(&mut self, sig: Iface, args: &mut IfaceCheckContext, pol: bool) -> Option<()>;
    fn check_var(&mut self, _var: &Interned<TypeVar>, _pol: bool) -> Option<TypeBounds> {
        None
    }
}

impl<T> IfaceChecker for T
where
    T: FnMut(Iface, &mut IfaceCheckContext, bool) -> Option<()>,
{
    fn check(&mut self, sig: Iface, args: &mut IfaceCheckContext, pol: bool) -> Option<()> {
        self(sig, args, pol)
    }
}

impl Ty {
    /// Iterate over the signatures of the given type.
    pub fn iface_surface(
        &self,
        pol: bool,
        // iface_kind: IfaceSurfaceKind,
        checker: &mut impl IfaceChecker,
    ) {
        let context = IfaceCheckContext {
            args: Vec::new(),
            at: TyRef::new(Ty::Any),
        };
        let mut worker = IfaceCheckDriver {
            ctx: context,
            checker,
        };

        worker.ty(self, pol);
    }
}

pub struct IfaceCheckContext {
    pub args: Vec<Interned<SigTy>>,
    pub at: TyRef,
}

pub struct IfaceCheckDriver<'a> {
    ctx: IfaceCheckContext,
    checker: &'a mut dyn IfaceChecker,
}

impl BoundChecker for IfaceCheckDriver<'_> {
    fn collect(&mut self, ty: &Ty, pol: bool) {
        self.ty(ty, pol);
    }

    fn bound_of_var(&mut self, var: &Interned<TypeVar>, pol: bool) -> Option<TypeBounds> {
        self.checker.check_var(var, pol)
    }
}

impl<'a> IfaceCheckDriver<'a> {
    fn dict_as_iface(&self) -> bool {
        // matches!(
        // self.ctx.sig_kind,
        // SigSurfaceKind::DictIface | SigSurfaceKind::ArrayOrDict
        // )
        true
    }

    fn value_as_iface(&self) -> bool {
        // matches!(self.ctx.sig_kind, SigSurfaceKind::Func)
        true
    }

    fn ty(&mut self, ty: &Ty, pol: bool) {
        println!("check iface ty: {ty:?}");

        match ty {
            Ty::Builtin(BuiltinTy::Stroke) if self.dict_as_iface() => {
                self.checker
                    .check(Iface::Dict(&FLOW_STROKE_DICT), &mut self.ctx, pol);
            }
            Ty::Builtin(BuiltinTy::Margin) if self.dict_as_iface() => {
                self.checker
                    .check(Iface::Dict(&FLOW_MARGIN_DICT), &mut self.ctx, pol);
            }
            Ty::Builtin(BuiltinTy::Inset) if self.dict_as_iface() => {
                self.checker
                    .check(Iface::Dict(&FLOW_INSET_DICT), &mut self.ctx, pol);
            }
            Ty::Builtin(BuiltinTy::Outset) if self.dict_as_iface() => {
                self.checker
                    .check(Iface::Dict(&FLOW_OUTSET_DICT), &mut self.ctx, pol);
            }
            Ty::Builtin(BuiltinTy::Radius) if self.dict_as_iface() => {
                self.checker
                    .check(Iface::Dict(&FLOW_RADIUS_DICT), &mut self.ctx, pol);
            }
            // // todo: deduplicate checking early
            Ty::Value(v) => {
                if self.value_as_iface() {
                    match &v.val {
                        // Value::Func(f) => {
                        //     self.checker
                        //         .check(Iface::Value { val: f, at: ty }, &mut self.ctx, pol);
                        // }
                        Value::Dict(d) => {
                            self.checker
                                .check(Iface::Value { val: d, at: ty }, &mut self.ctx, pol);
                        }
                        Value::Type(t) => {
                            self.checker
                                .check(Iface::Type { val: t, at: ty }, &mut self.ctx, pol);
                        }
                        _ => {}
                    }
                }
            }
            Ty::Builtin(BuiltinTy::Type(e)) if self.value_as_iface() => {
                // todo: distinguish between element and function
                self.checker
                    .check(Iface::Type { val: e, at: ty }, &mut self.ctx, pol);
            }
            Ty::Builtin(BuiltinTy::Element(e)) if self.value_as_iface() => {
                self.checker
                    .check(Iface::Element { val: e, at: ty }, &mut self.ctx, pol);
            }
            // Ty::Func(sig) if self.value_as_iface() => {
            //     self.checker.check(Iface::Type(sig), &mut self.ctx, pol);
            // }
            // Ty::Array(sig) if self.array_as_sig() => {
            //     // let sig = FlowSignature::array_cons(*sig.clone(), true);
            //     self.checker.check(Iface::ArrayCons(sig), &mut self.ctx, pol);
            // }
            // // todo: tuple
            // Ty::Tuple(_) => {}
            Ty::Dict(sig) if self.dict_as_iface() => {
                // self.check_dict_signature(sig, pol, self.checker);
                self.checker.check(Iface::Dict(sig), &mut self.ctx, pol);
            }
            _ if ty.has_bounds() => ty.bounds(pol, self),
            _ => {}
        }
        // Ty::Select(sel) => sel.ty.bounds(pol, &mut MethodDriver(self,
        // &sel.select)), // todo: calculate these operators
        // Ty::Unary(_) => {}
        // Ty::Binary(_) => {}
        // Ty::If(_) => {}
    }
}
