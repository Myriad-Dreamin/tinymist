use reflexo_typst::TypstFileId;
use typst::foundations::{Dict, Module};

use super::BoundChecker;
use crate::{syntax::Decl, ty::prelude::*};

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
    Module {
        val: TypstFileId,
        at: &'a Ty,
    },
    ModuleVal {
        val: &'a Module,
        at: &'a Ty,
    },
}

impl Iface<'_> {
    // IfaceShape { iface }
    pub fn select(self, ctx: &mut impl TyCtxMut, key: &StrRef) -> Option<Ty> {
        crate::log_debug_ct!("iface shape: {self:?}");

        match self {
            // Iface::ArrayCons(a) => SigTy::array_cons(a.as_ref().clone(), false),
            Iface::Dict(d) => d.field_by_name(key).cloned(),
            // Iface::Type { val, .. } => ctx?.type_of_func(&val.constructor().ok()?)?,
            // Iface::Value { val, .. } => ctx?.type_of_func(val)?, // todo
            Iface::Element { .. } => None,
            Iface::Type { .. } => None,
            Iface::Value { val, at: _ } => ctx.type_of_dict(val).field_by_name(key).cloned(),
            Iface::Module { val, at: _ } => ctx.check_module_item(val, key),
            Iface::ModuleVal { val, at: _ } => ctx.type_of_module(val).field_by_name(key).cloned(),
        }
    }
}

pub trait IfaceChecker: TyCtx {
    fn check(&mut self, iface: Iface, ctx: &mut IfaceCheckContext, pol: bool) -> Option<()>;
}

impl Ty {
    /// Iterate over the signatures of the given type.
    pub fn iface_surface(
        &self,
        pol: bool,
        // iface_kind: IfaceSurfaceKind,
        checker: &mut impl IfaceChecker,
    ) {
        let context = IfaceCheckContext { args: Vec::new() };
        let mut worker = IfaceCheckDriver {
            ctx: context,
            checker,
        };

        worker.ty(self, pol);
    }
}

pub struct IfaceCheckContext {
    pub args: Vec<Interned<SigTy>>,
}

#[derive(BindTyCtx)]
#[bind(checker)]
pub struct IfaceCheckDriver<'a> {
    ctx: IfaceCheckContext,
    checker: &'a mut dyn IfaceChecker,
}

impl BoundChecker for IfaceCheckDriver<'_> {
    fn collect(&mut self, ty: &Ty, pol: bool) {
        self.ty(ty, pol);
    }
}

impl IfaceCheckDriver<'_> {
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
        crate::log_debug_ct!("check iface ty: {ty:?}");

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
                        Value::Module(t) => {
                            self.checker.check(
                                Iface::ModuleVal { val: t, at: ty },
                                &mut self.ctx,
                                pol,
                            );
                        }
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
            Ty::Builtin(BuiltinTy::Module(e)) => {
                if let Decl::Module(m) = e.as_ref() {
                    self.checker
                        .check(Iface::Module { val: m.fid, at: ty }, &mut self.ctx, pol);
                }
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
            Ty::Var(..) => ty.bounds(pol, self),
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
