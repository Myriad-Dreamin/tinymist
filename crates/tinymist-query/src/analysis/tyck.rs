//! Type checking on source file

use std::sync::OnceLock;

use rustc_hash::{FxHashMap, FxHashSet};
use tinymist_derive::BindTyCtx;

use super::{
    prelude::*, BuiltinTy, DynTypeBounds, FlowVarKind, SharedContext, TyCtxMut, TypeInfo, TypeVar,
    TypeVarBounds,
};
use crate::{
    syntax::{Decl, DeclExpr, Expr, ExprInfo, UnaryOp},
    ty::*,
};

mod apply;
mod convert;
mod docs;
mod select;
mod syntax;

pub(crate) use apply::*;
pub(crate) use convert::*;
pub(crate) use select::*;

#[derive(Default)]
pub struct TypeEnv {
    visiting: FxHashMap<TypstFileId, Arc<TypeInfo>>,
    exprs: FxHashMap<TypstFileId, Option<Arc<ExprInfo>>>,
}

/// Type checking at the source unit level.
pub(crate) fn type_check(
    ctx: Arc<SharedContext>,
    ei: Arc<ExprInfo>,
    env: &mut TypeEnv,
) -> Arc<TypeInfo> {
    let mut info = TypeInfo::default();
    info.valid = true;
    info.fid = Some(ei.fid);
    info.revision = ei.revision;

    env.visiting.insert(ei.fid, Arc::new(TypeInfo::default()));

    // Retrieve expression information for the source.
    let root = ei.root.clone();

    let mut checker = TypeChecker {
        ctx,
        ei,
        info,
        env,
        call_cache: Default::default(),
        module_exports: Default::default(),
    };

    let type_check_start = std::time::Instant::now();

    checker.check(&root);

    let exports = checker
        .ei
        .exports
        .clone()
        .into_iter()
        .map(|(k, v)| (k.clone(), checker.check(v)))
        .collect();
    checker.info.exports = exports;

    let elapsed = type_check_start.elapsed();
    crate::log_debug_ct!("Type checking on {:?} took {elapsed:?}", checker.ei.fid);

    checker.env.visiting.remove(&checker.ei.fid);

    Arc::new(checker.info)
}

type CallCacheDesc = (
    Interned<SigTy>,
    Interned<SigTy>,
    Option<Vec<Interned<SigTy>>>,
);

pub(crate) struct TypeChecker<'a> {
    ctx: Arc<SharedContext>,
    ei: Arc<ExprInfo>,

    info: TypeInfo,
    module_exports: FxHashMap<(TypstFileId, Interned<str>), OnceLock<Option<Ty>>>,

    call_cache: FxHashSet<CallCacheDesc>,

    env: &'a mut TypeEnv,
}

impl TyCtx for TypeChecker<'_> {
    fn global_bounds(&self, var: &Interned<TypeVar>, pol: bool) -> Option<DynTypeBounds> {
        self.info.global_bounds(var, pol)
    }

    fn local_bind_of(&self, var: &Interned<TypeVar>) -> Option<Ty> {
        self.info.local_bind_of(var)
    }
}

impl TyCtxMut for TypeChecker<'_> {
    type Snap = <TypeInfo as TyCtxMut>::Snap;

    fn start_scope(&mut self) -> Self::Snap {
        self.info.start_scope()
    }

    fn end_scope(&mut self, snap: Self::Snap) {
        self.info.end_scope(snap)
    }

    fn bind_local(&mut self, var: &Interned<TypeVar>, ty: Ty) {
        self.info.bind_local(var, ty);
    }

    fn type_of_func(&mut self, func: &Func) -> Option<Interned<SigTy>> {
        Some(self.ctx.type_of_func(func.clone()).type_sig())
    }

    fn type_of_value(&mut self, val: &Value) -> Ty {
        self.ctx.type_of_value(val)
    }

    fn check_module_item(&mut self, fid: TypstFileId, k: &StrRef) -> Option<Ty> {
        self.module_exports
            .entry((fid, k.clone()))
            .or_default()
            .clone()
            .get_or_init(|| {
                let ei = self
                    .env
                    .exprs
                    .entry(fid)
                    .or_insert_with(|| self.ctx.expr_stage_by_id(fid))
                    .clone()?;

                Some(self.check(ei.exports.get(k)?))
            })
            .clone()
    }
}

impl TypeChecker<'_> {
    fn check(&mut self, expr: &Expr) -> Ty {
        self.check_syntax(expr).unwrap_or(Ty::undef())
    }

    fn copy_doc_vars(
        &mut self,
        fr: &TypeVarBounds,
        var: &Interned<TypeVar>,
        base: &Interned<Decl>,
    ) -> Ty {
        let mut gen_var = var.as_ref().clone();
        let encoded = Interned::new(Decl::docs(base.clone(), var.clone()));
        gen_var.def = encoded.clone();
        crate::log_debug_ct!("copy var {fr:?} as {encoded:?}");
        let bounds = TypeVarBounds::new(gen_var, fr.bounds.bounds().read().clone());
        let var = bounds.as_type();
        self.info.vars.insert(encoded, bounds);
        var
    }

    fn get_var(&mut self, decl: &DeclExpr) -> Interned<TypeVar> {
        crate::log_debug_ct!("get_var {decl:?}");
        let entry = self.info.vars.entry(decl.clone()).or_insert_with(|| {
            let name = decl.name().clone();
            let decl = decl.clone();

            // Check External variables
            let init = decl.file_id().and_then(|fid| {
                if fid == self.ei.fid {
                    return None;
                }

                crate::log_debug_ct!("import_ty {name} from {fid:?}");

                let ext_def_use_info = self.ctx.expr_stage_by_id(fid)?;
                let source = &ext_def_use_info.source;
                // todo: check types in cycle
                let ext_type_info = if let Some(scheme) = self.env.visiting.get(&source.id()) {
                    scheme.clone()
                } else {
                    self.ctx.clone().type_check_(source, self.env)
                };
                let ext_def = ext_def_use_info.exports.get(&name)?;

                // todo: rest expressions
                let def = match ext_def {
                    Expr::Decl(decl) => {
                        let ext_ty = ext_type_info.vars.get(decl)?.as_type();
                        if let Some(ext_docs) = ext_type_info.var_docs.get(decl) {
                            self.info.var_docs.insert(decl.clone(), ext_docs.clone());
                        }

                        ext_type_info.simplify(ext_ty, false)
                    }
                    _ => return None,
                };

                Some(ext_type_info.to_bounds(def))
            });

            TypeVarBounds::new(TypeVar { name, def: decl }, init.unwrap_or_default())
        });

        let var = entry.var.clone();

        let s = decl.span();
        if !s.is_detached() {
            // todo: record decl types
            // let should_record = matches!(root.kind(), SyntaxKind::FuncCall).then(||
            // root.span());
            // if let Some(s) = should_record {
            //     self.info.witness_at_least(s, w.clone());
            // }

            TypeInfo::witness_(s, Ty::Var(var.clone()), &mut self.info.mapping);
        }
        var
    }

    fn constrain_call(
        &mut self,
        sig: &Interned<SigTy>,
        args: &Interned<SigTy>,
        withs: Option<&Vec<Interned<SigTy>>>,
    ) {
        let call_desc = (sig.clone(), args.clone(), withs.cloned());
        if !self.call_cache.insert(call_desc) {
            return;
        }

        for (arg_recv, arg_ins) in sig.matches(args, withs) {
            self.constrain(arg_ins, arg_recv);
        }
    }

    fn constrain(&mut self, lhs: &Ty, rhs: &Ty) {
        static FLOW_STROKE_DICT_TYPE: LazyLock<Ty> =
            LazyLock::new(|| Ty::Dict(FLOW_STROKE_DICT.clone()));
        static FLOW_MARGIN_DICT_TYPE: LazyLock<Ty> =
            LazyLock::new(|| Ty::Dict(FLOW_MARGIN_DICT.clone()));
        static FLOW_INSET_DICT_TYPE: LazyLock<Ty> =
            LazyLock::new(|| Ty::Dict(FLOW_INSET_DICT.clone()));
        static FLOW_OUTSET_DICT_TYPE: LazyLock<Ty> =
            LazyLock::new(|| Ty::Dict(FLOW_OUTSET_DICT.clone()));
        static FLOW_RADIUS_DICT_TYPE: LazyLock<Ty> =
            LazyLock::new(|| Ty::Dict(FLOW_RADIUS_DICT.clone()));

        fn is_ty(ty: &Ty) -> bool {
            match ty {
                Ty::Builtin(BuiltinTy::Type(..)) => true,
                Ty::Value(val) => matches!(val.val, Value::Type(..)),
                _ => false,
            }
        }

        if lhs == rhs {
            return;
        }

        match (lhs, rhs) {
            (Ty::Var(v), Ty::Var(w)) => {
                if v.def == w.def {
                    return;
                }

                // todo: merge

                let _ = v.def;
                let _ = w.def;
            }
            (Ty::Var(v), rhs) => {
                crate::log_debug_ct!("constrain var {v:?} ⪯ {rhs:?}");
                let w = self.info.vars.get_mut(&v.def).unwrap();
                // strict constraint on upper bound
                let bound = rhs.clone();
                match &w.bounds {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let mut w = w.write();
                        w.ubs.insert_mut(bound);
                    }
                }
            }
            (lhs, Ty::Var(v)) => {
                let w = self.info.vars.get(&v.def).unwrap();
                let bound = self.weaken_constraint(lhs, &w.bounds);
                crate::log_debug_ct!("constrain var {v:?} ⪰ {bound:?}");
                match &w.bounds {
                    FlowVarKind::Strong(v) | FlowVarKind::Weak(v) => {
                        let mut v = v.write();
                        v.lbs.insert_mut(bound);
                    }
                }
            }
            (Ty::Union(types), rhs) => {
                for ty in types.iter() {
                    self.constrain(ty, rhs);
                }
            }
            (lhs, Ty::Union(types)) => {
                for ty in types.iter() {
                    self.constrain(lhs, ty);
                }
            }
            (lhs, Ty::Builtin(BuiltinTy::Stroke)) => {
                // empty array is also a constructing dict but we can safely ignore it during
                // type checking, since no fields are added yet.
                if lhs.is_dict() {
                    self.constrain(lhs, &FLOW_STROKE_DICT_TYPE);
                }
            }
            (Ty::Builtin(BuiltinTy::Stroke), rhs) => {
                if rhs.is_dict() {
                    self.constrain(&FLOW_STROKE_DICT_TYPE, rhs);
                }
            }
            (lhs, Ty::Builtin(BuiltinTy::Margin)) => {
                if lhs.is_dict() {
                    self.constrain(lhs, &FLOW_MARGIN_DICT_TYPE);
                }
            }
            (Ty::Builtin(BuiltinTy::Margin), rhs) => {
                if rhs.is_dict() {
                    self.constrain(&FLOW_MARGIN_DICT_TYPE, rhs);
                }
            }
            (lhs, Ty::Builtin(BuiltinTy::Inset)) => {
                if lhs.is_dict() {
                    self.constrain(lhs, &FLOW_INSET_DICT_TYPE);
                }
            }
            (Ty::Builtin(BuiltinTy::Inset), rhs) => {
                if rhs.is_dict() {
                    self.constrain(&FLOW_INSET_DICT_TYPE, rhs);
                }
            }
            (lhs, Ty::Builtin(BuiltinTy::Outset)) => {
                if lhs.is_dict() {
                    self.constrain(lhs, &FLOW_OUTSET_DICT_TYPE);
                }
            }
            (Ty::Builtin(BuiltinTy::Outset), rhs) => {
                if rhs.is_dict() {
                    self.constrain(&FLOW_OUTSET_DICT_TYPE, rhs);
                }
            }
            (lhs, Ty::Builtin(BuiltinTy::Radius)) => {
                if lhs.is_dict() {
                    self.constrain(lhs, &FLOW_RADIUS_DICT_TYPE);
                }
            }
            (Ty::Builtin(BuiltinTy::Radius), rhs) => {
                if rhs.is_dict() {
                    self.constrain(&FLOW_RADIUS_DICT_TYPE, rhs);
                }
            }
            (Ty::Dict(lhs), Ty::Dict(rhs)) => {
                for (key, lhs, rhs) in lhs.common_iface_fields(rhs) {
                    crate::log_debug_ct!("constrain record item {key} {lhs:?} ⪯ {rhs:?}");
                    self.constrain(lhs, rhs);
                    // if !sl.is_detached() {
                    //     self.info.witness_at_most(*sl, rhs.clone());
                    // }
                    // if !sr.is_detached() {
                    //     self.info.witness_at_least(*sr, lhs.clone());
                    // }
                }
            }
            (Ty::Unary(lhs), Ty::Unary(rhs)) if lhs.op == rhs.op => {
                // todo: more information could be extracted from unary constraint structure
                // e.g. type(l) == type(r)
                self.constrain(&lhs.lhs, &rhs.lhs);
            }
            (Ty::Unary(lhs), rhs) if lhs.op == UnaryOp::TypeOf && is_ty(rhs) => {
                crate::log_debug_ct!("constrain type of {lhs:?} ⪯ {rhs:?}");

                self.constrain(&lhs.lhs, rhs);
            }
            (lhs, Ty::Unary(rhs)) if rhs.op == UnaryOp::TypeOf && is_ty(lhs) => {
                crate::log_debug_ct!(
                    "constrain type of {lhs:?} ⪯ {rhs:?} {:?}",
                    matches!(lhs, Ty::Builtin(..)),
                );
                self.constrain(lhs, &rhs.lhs);
            }
            (Ty::Value(lhs), rhs) => {
                crate::log_debug_ct!("constrain value {lhs:?} ⪯ {rhs:?}");
                let _ = TypeInfo::witness_at_most;
                // if !lhs.1.is_detached() {
                //     self.info.witness_at_most(lhs.1, rhs.clone());
                // }
            }
            (lhs, Ty::Value(rhs)) => {
                crate::log_debug_ct!("constrain value {lhs:?} ⪯ {rhs:?}");
                // if !rhs.1.is_detached() {
                //     self.info.witness_at_least(rhs.1, lhs.clone());
                // }
            }
            _ => {
                crate::log_debug_ct!("constrain {lhs:?} ⪯ {rhs:?}");
            }
        }
    }

    fn check_comparable(&self, lhs: &Ty, rhs: &Ty) {
        let _ = lhs;
        let _ = rhs;
    }

    fn check_assignable(&self, lhs: &Ty, rhs: &Ty) {
        let _ = lhs;
        let _ = rhs;
    }

    fn check_containing(&mut self, container: &Ty, elem: &Ty, expected_in: bool) {
        let rhs = if expected_in {
            match container {
                Ty::Tuple(elements) => Ty::Union(elements.clone()),
                _ => Ty::Unary(TypeUnary::new(UnaryOp::ElementOf, container.clone())),
            }
        } else {
            // todo: remove not element of
            Ty::Unary(TypeUnary::new(UnaryOp::NotElementOf, container.clone()))
        };

        self.constrain(elem, &rhs);
    }

    fn possible_ever_be(&mut self, lhs: &Ty, rhs: &Ty) {
        // todo: instantiataion
        match rhs {
            Ty::Builtin(..) | Ty::Value(..) | Ty::Boolean(..) => {
                self.constrain(rhs, lhs);
            }
            _ => {}
        }
    }

    fn weaken(&mut self, v: &Ty) {
        match v {
            Ty::Var(v) => {
                let w = self.info.vars.get_mut(&v.def).unwrap();
                w.weaken();
            }
            Ty::Any | Ty::Boolean(_) | Ty::Builtin(_) | Ty::Value(_) => {}
            Ty::Param(v) => {
                self.weaken(&v.ty);
            }
            Ty::Func(v) | Ty::Args(v) | Ty::Pattern(v) => {
                for ty in v.inputs() {
                    self.weaken(ty);
                }
            }
            Ty::With(v) => {
                self.weaken(&v.sig);
                for ty in v.with.inputs() {
                    self.weaken(ty);
                }
            }
            Ty::Dict(v) => {
                for (_, ty) in v.interface() {
                    self.weaken(ty);
                }
            }
            Ty::Array(v) => {
                self.weaken(v);
            }
            Ty::Tuple(v) => {
                for ty in v.iter() {
                    self.weaken(ty);
                }
            }
            Ty::Select(v) => {
                self.weaken(&v.ty);
            }
            Ty::Unary(v) => {
                self.weaken(&v.lhs);
            }
            Ty::Binary(v) => {
                let [lhs, rhs] = v.operands();
                self.weaken(lhs);
                self.weaken(rhs);
            }
            Ty::If(v) => {
                self.weaken(&v.cond);
                self.weaken(&v.then);
                self.weaken(&v.else_);
            }
            Ty::Union(v) => {
                for ty in v.iter() {
                    self.weaken(ty);
                }
            }
            Ty::Let(v) => {
                for ty in v.lbs.iter() {
                    self.weaken(ty);
                }
                for ty in v.ubs.iter() {
                    self.weaken(ty);
                }
            }
        }
    }

    fn weaken_constraint(&self, c: &Ty, kind: &FlowVarKind) -> Ty {
        if matches!(kind, FlowVarKind::Strong(_)) {
            return c.clone();
        }

        if let Ty::Value(v) = c {
            return BuiltinTy::from_value(&v.val);
        }

        c.clone()
    }
}

struct Joiner {
    break_or_continue_or_return: bool,
    definite: Ty,
    possibles: Vec<Ty>,
}
impl Joiner {
    fn finalize(self) -> Ty {
        crate::log_debug_ct!("join: {:?} {:?}", self.possibles, self.definite);
        if self.possibles.is_empty() {
            return self.definite;
        }
        if self.possibles.len() == 1 {
            return self.possibles.into_iter().next().unwrap();
        }

        // let mut definite = self.definite.clone();
        // for p in &self.possibles {
        //     definite = definite.join(p);
        // }

        // crate::log_debug_ct!("possibles: {:?} {:?}", self.definite, self.possibles);

        Ty::Any
    }

    fn join(&mut self, child: Ty) {
        if self.break_or_continue_or_return {
            return;
        }

        match (child, &self.definite) {
            (Ty::Builtin(BuiltinTy::Space | BuiltinTy::None), _) => {}
            (Ty::Builtin(BuiltinTy::Clause | BuiltinTy::FlowNone), _) => {}
            (Ty::Any, _) | (_, Ty::Any) => {}
            (Ty::Var(var), _) => self.possibles.push(Ty::Var(var)),
            // todo: check possibles
            (Ty::Array(arr), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Array(arr),
            (Ty::Array(..), _) => self.definite = Ty::undef(),
            (Ty::Tuple(elems), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Tuple(elems),
            (Ty::Tuple(..), _) => self.definite = Ty::undef(),
            // todo: mystery flow none
            // todo: possible some style (auto)
            (Ty::Builtin(ty), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Builtin(ty),
            (Ty::Builtin(..), _) => self.definite = Ty::undef(),
            // todo: value join
            (Ty::Value(v), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Value(v),
            (Ty::Value(..), _) => self.definite = Ty::undef(),
            (Ty::Func(f), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Func(f),
            (Ty::Func(..), _) => self.definite = Ty::undef(),
            (Ty::Dict(w), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Dict(w),
            (Ty::Dict(..), _) => self.definite = Ty::undef(),
            (Ty::With(w), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::With(w),
            (Ty::With(..), _) => self.definite = Ty::undef(),
            (Ty::Args(w), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Args(w),
            (Ty::Args(..), _) => self.definite = Ty::undef(),
            (Ty::Pattern(w), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Pattern(w),
            (Ty::Pattern(..), _) => self.definite = Ty::undef(),
            (Ty::Select(w), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Select(w),
            (Ty::Select(..), _) => self.definite = Ty::undef(),
            (Ty::Unary(w), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Unary(w),
            (Ty::Unary(..), _) => self.definite = Ty::undef(),
            (Ty::Binary(w), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Binary(w),
            (Ty::Binary(..), _) => self.definite = Ty::undef(),
            (Ty::If(w), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::If(w),
            (Ty::If(..), _) => self.definite = Ty::undef(),
            (Ty::Union(w), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Union(w),
            (Ty::Union(..), _) => self.definite = Ty::undef(),
            (Ty::Let(w), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Let(w),
            (Ty::Let(..), _) => self.definite = Ty::undef(),
            (Ty::Param(w), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Param(w),
            (Ty::Param(..), _) => self.definite = Ty::undef(),
            (Ty::Boolean(b), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Boolean(b),
            (Ty::Boolean(..), _) => self.definite = Ty::undef(),
        }
    }
}
impl Default for Joiner {
    fn default() -> Self {
        Self {
            break_or_continue_or_return: false,
            definite: Ty::Builtin(BuiltinTy::None),
            possibles: Vec::new(),
        }
    }
}
