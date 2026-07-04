//! Type checking on source file

use std::sync::OnceLock;

use rustc_hash::{FxHashMap, FxHashSet};
use tinymist_derive::BindTyCtx;
use tinymist_std::DefId;
use typst::syntax::Span;

use super::{
    BuiltinTy, DynTypeBounds, FlowVarKind, SharedContext, TyCtxMut, TypeInfo, TypeVar,
    TypeVarBounds, prelude::*,
};
use crate::{
    syntax::{Decl, DeclExpr, DefKind, Expr, ExprInfo, FuncExpr, UnaryOp},
    ty::*,
};

mod apply;
mod docs;
mod select;
mod syntax;

pub(crate) use apply::*;
pub(crate) use select::*;

#[derive(Default)]
pub struct TypeEnv {
    visiting: FxHashMap<TypstFileId, Arc<TypeInfo>>,
    checked: FxHashMap<TypstFileId, Arc<TypeInfo>>,
    precise_signature_seeds: FxHashMap<TypstFileId, Arc<PreciseSignatureSeed>>,
    precise_signatures: FxHashMap<(TypstFileId, DeclExpr), Arc<TypeInfo>>,
    exprs: FxHashMap<TypstFileId, Option<ExprInfo>>,
}

/// Type checking at the source unit level.
#[typst_macros::time(span = ei.source.root().span())]
pub(crate) fn type_check(
    ctx: &Arc<SharedContext>,
    ei: ExprInfo,
    env: &mut TypeEnv,
) -> Arc<TypeInfo> {
    env.visiting.insert(ei.fid, Arc::new(TypeInfo::default()));

    // Retrieve expression information for the source.
    let root = ei.root.clone();

    let mut checker = TypeChecker::new(ctx, ei, env);

    let type_check_start = tinymist_std::time::Instant::now();

    checker.check(&root);
    checker.infer_deferred_func_bodies();

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

    let fid = checker.ei.fid;
    checker.env.visiting.remove(&fid);

    let info = Arc::new(std::mem::take(&mut checker.info));
    checker.env.checked.insert(fid, info.clone());

    info
}

/// Type checks a source locally for a function signature query.
///
/// This returns precise signature information, including constraints learned
/// from function bodies, but it intentionally does not go through the shared
/// file-level `type_check` cache for the queried source.
pub(crate) fn type_check_for_precise_signature(
    ctx: &Arc<SharedContext>,
    ei: ExprInfo,
    def: &DeclExpr,
) -> Option<Arc<TypeInfo>> {
    let mut env = TypeEnv::default();
    type_check_for_precise_signature_(ctx, ei, def, &mut env)
}

fn type_check_for_precise_signature_(
    ctx: &Arc<SharedContext>,
    ei: ExprInfo,
    def: &DeclExpr,
    env: &mut TypeEnv,
) -> Option<Arc<TypeInfo>> {
    let fid = ei.fid;
    let cache_key = (fid, def.clone());
    if let Some(cached) = env.precise_signatures.get(&cache_key).cloned() {
        return Some(cached);
    }

    env.visiting.insert(fid, Arc::new(TypeInfo::default()));
    let seed = if let Some(seed) = env.precise_signature_seeds.get(&fid).cloned() {
        seed
    } else {
        let root = ei.root.clone();
        let mut checker = TypeChecker::new(ctx, ei.clone(), env);
        checker.check(&root);
        let seed = Arc::new(PreciseSignatureSeed::from_checker(&checker));
        checker
            .env
            .precise_signature_seeds
            .insert(fid, seed.clone());
        seed
    };

    let mut checker = TypeChecker::from_precise_signature_seed(ctx, ei, env, &seed);
    checker.infer_deferred_func_body_by_decl(def);
    checker.env.visiting.remove(&fid);

    let info = Arc::new(checker.info);
    checker
        .env
        .precise_signatures
        .insert(cache_key, info.clone());
    Some(info)
}

type CallCacheDesc = (
    Interned<SigTy>,
    Interned<SigTy>,
    Option<Vec<Interned<SigTy>>>,
);

#[derive(Clone)]
struct PreciseSignatureSeed {
    info: TypeInfo,
    call_cache: FxHashSet<CallCacheDesc>,
    deferred_func_bodies: Vec<DeferredFuncBody>,
    deferred_func_returns: FxHashMap<DeclExpr, Ty>,
    deferred_calls: Vec<DeferredCall>,
    deferred_call_cache: FxHashMap<CallCacheDesc, Ty>,
    next_generated: u64,
}

const MAX_RESULTANT_INSTANTIATION_DEPTH: usize = 16;
const MAX_RESULTANT_INSTANTIATION_STEPS: usize = 4096;
const MAX_RESULTANT_INSTANTIATION_WIDTH: usize = 32;

pub(crate) struct TypeChecker<'a> {
    ctx: &'a Arc<SharedContext>,
    ei: ExprInfo,

    info: TypeInfo,
    module_exports: FxHashMap<(TypstFileId, Interned<str>), OnceLock<Option<Ty>>>,

    call_cache: FxHashSet<CallCacheDesc>,
    deferred_func_bodies: Vec<DeferredFuncBody>,
    deferred_func_returns: FxHashMap<DeclExpr, Ty>,
    deferred_calls: Vec<DeferredCall>,
    deferred_call_cache: FxHashMap<CallCacheDesc, Ty>,
    resultant_visiting: FxHashSet<DeclExpr>,
    checking_deferred_body: bool,
    record_witnesses: bool,
    inferring_func_returns: Vec<Ty>,
    next_generated: u64,

    env: &'a mut TypeEnv,
}

#[derive(Clone)]
pub(crate) struct DeferredFuncBody {
    func: Interned<FuncExpr>,
    ret: Ty,
    blocked_returns: Vec<Ty>,
    default_infer: bool,
}

#[derive(Clone)]
pub(crate) struct DeferredCall {
    sig: Interned<SigTy>,
    args: Interned<ArgsTy>,
    withs: Option<Vec<Interned<SigTy>>>,
    ret: Ty,
}

impl PreciseSignatureSeed {
    fn from_checker(checker: &TypeChecker<'_>) -> Self {
        Self {
            info: checker.info.clone(),
            call_cache: checker.call_cache.clone(),
            deferred_func_bodies: checker.deferred_func_bodies.clone(),
            deferred_func_returns: checker.deferred_func_returns.clone(),
            deferred_calls: checker.deferred_calls.clone(),
            deferred_call_cache: checker.deferred_call_cache.clone(),
            next_generated: checker.next_generated,
        }
    }
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

    fn check_module_item(&mut self, fid: TypstFileId, name: &StrRef) -> Option<Ty> {
        self.module_exports
            .entry((fid, name.clone()))
            .or_default()
            .clone()
            .get_or_init(|| {
                let ei = self
                    .env
                    .exprs
                    .entry(fid)
                    .or_insert_with(|| self.ctx.expr_stage_by_id(fid))
                    .clone()?;

                Some(self.check(ei.exports.get(name)?))
            })
            .clone()
    }
}

impl TypeChecker<'_> {
    fn new<'a>(
        ctx: &'a Arc<SharedContext>,
        ei: ExprInfo,
        env: &'a mut TypeEnv,
    ) -> TypeChecker<'a> {
        let mut info = TypeInfo::default();
        info.valid = true;
        info.fid = Some(ei.fid);
        info.revision = ei.revision;

        TypeChecker {
            ctx,
            ei,
            info,
            env,
            call_cache: Default::default(),
            module_exports: Default::default(),
            deferred_func_bodies: Default::default(),
            deferred_func_returns: Default::default(),
            deferred_calls: Default::default(),
            deferred_call_cache: Default::default(),
            resultant_visiting: Default::default(),
            checking_deferred_body: false,
            record_witnesses: true,
            inferring_func_returns: Default::default(),
            next_generated: 0,
        }
    }

    fn from_precise_signature_seed<'a>(
        ctx: &'a Arc<SharedContext>,
        ei: ExprInfo,
        env: &'a mut TypeEnv,
        seed: &PreciseSignatureSeed,
    ) -> TypeChecker<'a> {
        TypeChecker {
            ctx,
            ei,
            info: seed.info.clone(),
            env,
            call_cache: seed.call_cache.clone(),
            module_exports: Default::default(),
            deferred_func_bodies: seed.deferred_func_bodies.clone(),
            deferred_func_returns: seed.deferred_func_returns.clone(),
            deferred_calls: seed.deferred_calls.clone(),
            deferred_call_cache: seed.deferred_call_cache.clone(),
            resultant_visiting: Default::default(),
            checking_deferred_body: false,
            record_witnesses: true,
            inferring_func_returns: Default::default(),
            next_generated: seed.next_generated,
        }
    }

    pub(super) fn fresh_generated_var(&mut self, name: Interned<str>) -> Ty {
        self.next_generated += 1;
        let encoded = Interned::new(Decl::generated(DefId(
            0x7479_636b_0000_0000u64 + self.next_generated,
        )));
        let var = TypeVar {
            name,
            def: encoded.clone(),
        };
        let bounds = TypeVarBounds::new(var, DynTypeBounds::default());
        let ty = bounds.as_type();
        self.info.vars.insert(encoded, bounds);
        ty
    }

    pub(super) fn fresh_return_var(&mut self, base: &DeclExpr) -> Ty {
        self.fresh_generated_var(base.name().clone())
    }

    pub(super) fn defer_func_body(&mut self, func: Interned<FuncExpr>, ret: Ty) {
        let default_infer = matches!(func.decl.as_ref(), Decl::Func(..) | Decl::Closure(..));
        self.deferred_func_bodies.push(DeferredFuncBody {
            func,
            ret,
            blocked_returns: self.inferring_func_returns.clone(),
            default_infer,
        });
    }

    pub(super) fn mark_deferred_func_body_default(&mut self, def: &DeclExpr) {
        if let Some(body) = self
            .deferred_func_bodies
            .iter_mut()
            .rev()
            .find(|body| &body.func.decl == def)
        {
            body.default_infer = true;
        }
    }

    pub(super) fn defer_call(
        &mut self,
        sig: Interned<SigTy>,
        args: Interned<ArgsTy>,
        withs: Option<Vec<Interned<SigTy>>>,
        ret: Ty,
    ) {
        let desc = (sig.clone(), args.clone(), withs.clone());
        if let Some(existing) = self.deferred_call_cache.get(&desc).cloned() {
            self.constrain(&existing, &ret);
            return;
        }

        self.deferred_call_cache.insert(desc, ret.clone());
        self.deferred_calls.push(DeferredCall {
            sig,
            args,
            withs,
            ret,
        });
    }

    fn infer_deferred_func_bodies(&mut self) {
        self.with_record_witnesses(false, |checker| {
            let _ = checker.infer_deferred_func_bodies_fixed_point(2);
        });

        let inferred = self.with_record_witnesses(true, |checker| {
            checker.infer_deferred_func_bodies_fixed_point(0)
        });

        self.weaken_inferred_func_bodies(&inferred);
    }

    fn infer_deferred_func_bodies_fixed_point(
        &mut self,
        mut remaining_rechecks: usize,
    ) -> FxHashSet<DeclExpr> {
        let mut body_cursor = 0;
        let mut call_cursor = 0;
        let mut inferred = FxHashSet::default();
        while body_cursor < self.deferred_func_bodies.len()
            || call_cursor < self.deferred_calls.len()
        {
            while body_cursor < self.deferred_func_bodies.len() {
                let body = self.deferred_func_bodies[body_cursor].clone();
                body_cursor += 1;
                if !body.default_infer {
                    continue;
                }
                self.infer_deferred_func_body(body, &mut inferred);
            }

            let call_start = call_cursor;
            while call_cursor < self.deferred_calls.len() {
                let DeferredCall {
                    sig,
                    args,
                    withs,
                    ret,
                } = self.deferred_calls[call_cursor].clone();
                call_cursor += 1;

                if Self::deferred_call_can_instantiate(&args, withs.as_ref()) {
                    if let Some(body) = sig.body.as_ref()
                        && let Some(def) = self.deferred_func_body_decl_for_ret(body)
                    {
                        self.infer_deferred_func_body_by_decl_(&def, &mut inferred);
                    }

                    if let Some(body) = self.instantiate_sig_result(&sig, &args, withs.as_ref()) {
                        self.constrain(&body, &ret);
                    }
                }
            }

            if call_cursor > call_start && remaining_rechecks > 0 {
                remaining_rechecks -= 1;
                body_cursor = 0;
                inferred.clear();
            }
        }

        inferred
    }

    fn with_record_witnesses<T>(
        &mut self,
        record_witnesses: bool,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let prev = self.record_witnesses;
        self.record_witnesses = record_witnesses;
        let res = f(self);
        self.record_witnesses = prev;
        res
    }

    pub(super) fn witness_at_least(&mut self, site: Span, ty: Ty) {
        if self.record_witnesses {
            self.info.witness_at_least(site, ty);
        }
    }

    pub(super) fn witness_at_most(&mut self, site: Span, ty: Ty) {
        if self.record_witnesses {
            self.info.witness_at_most(site, ty);
        }
    }

    pub(super) fn call_resultant(mut resultants: Vec<Ty>) -> Ty {
        resultants = resultants
            .into_iter()
            .map(Self::drop_imprecise_any)
            .collect();
        if resultants.iter().any(|ty| !matches!(ty, Ty::Any)) {
            resultants.retain(|ty| !matches!(ty, Ty::Any));
        }

        Ty::from_types(resultants.into_iter())
    }

    fn drop_imprecise_any(ty: Ty) -> Ty {
        match ty {
            Ty::Union(types) => {
                let mut types = types
                    .iter()
                    .cloned()
                    .map(Self::drop_imprecise_any)
                    .collect::<Vec<_>>();
                if types.iter().any(|ty| !matches!(ty, Ty::Any)) {
                    types.retain(|ty| !matches!(ty, Ty::Any));
                }
                Ty::from_types(types.into_iter())
            }
            ty => ty,
        }
    }

    fn witness_var(&mut self, site: Span, ty: Ty) {
        if self.record_witnesses {
            TypeInfo::witness_(site, ty, &mut self.info.mapping);
        }
    }

    fn infer_deferred_func_body_by_decl(&mut self, def: &DeclExpr) {
        let mut inferred = FxHashSet::default();
        self.infer_deferred_func_body_by_decl_(def, &mut inferred);

        let mut call_cursor = 0;
        while call_cursor < self.deferred_calls.len() {
            let DeferredCall {
                sig,
                args,
                withs,
                ret,
            } = self.deferred_calls[call_cursor].clone();
            call_cursor += 1;

            if let Some(body) = sig.body.as_ref()
                && let Some(def) = self.deferred_func_body_decl_for_ret(body)
            {
                self.infer_deferred_func_body_by_decl_(&def, &mut inferred);
            }

            if let Some(body) = self.instantiate_sig_result(&sig, &args, withs.as_ref()) {
                self.constrain(&body, &ret);
            }
        }

        self.weaken_inferred_func_bodies(&inferred);
    }

    fn weaken_inferred_func_bodies(&mut self, inferred: &FxHashSet<DeclExpr>) {
        let returns = self
            .deferred_func_bodies
            .iter()
            .filter(|body| inferred.contains(&body.func.decl))
            .map(|body| body.ret.clone())
            .collect::<Vec<_>>();

        for ret in returns {
            self.weaken(&ret);
        }
    }

    fn infer_deferred_func_body_by_decl_(
        &mut self,
        def: &DeclExpr,
        inferred: &mut FxHashSet<DeclExpr>,
    ) {
        if !inferred.insert(def.clone()) {
            return;
        }

        let Some(DeferredFuncBody {
            func,
            ret,
            blocked_returns,
            ..
        }) = self
            .deferred_func_bodies
            .iter()
            .find(|body| &body.func.decl == def)
            .cloned()
        else {
            return;
        };

        self.infer_func_body(&func.body, &ret, &blocked_returns);
    }

    fn infer_deferred_func_body(
        &mut self,
        body: DeferredFuncBody,
        inferred: &mut FxHashSet<DeclExpr>,
    ) {
        let DeferredFuncBody {
            func,
            ret,
            blocked_returns,
            ..
        } = body;
        if !inferred.insert(func.decl.clone()) {
            return;
        }

        self.infer_func_body(&func.body, &ret, &blocked_returns);
    }

    fn deferred_func_body_decl_for_ret(&self, ret: &Ty) -> Option<DeclExpr> {
        self.deferred_func_bodies
            .iter()
            .find(|body| &body.ret == ret)
            .map(|body| body.func.decl.clone())
    }

    fn infer_func_body(&mut self, body: &Expr, ret: &Ty, blocked_returns: &[Ty]) {
        self.clear_var_lower_bounds(ret);
        let prev_checking_deferred_body = self.checking_deferred_body;
        let prev_inferring_func_returns = std::mem::take(&mut self.inferring_func_returns);
        self.inferring_func_returns
            .extend(blocked_returns.iter().cloned());
        self.inferring_func_returns.push(ret.clone());
        self.checking_deferred_body = true;
        let body = self.check_func_body_result(body);
        self.checking_deferred_body = prev_checking_deferred_body;
        self.inferring_func_returns = prev_inferring_func_returns;
        let body = Self::function_result(body);
        self.constrain(&body, ret);
    }

    fn clear_var_lower_bounds(&mut self, ty: &Ty) {
        let Ty::Var(var) = ty else {
            return;
        };
        let Some(bounds) = self.info.vars.get_mut(&var.def) else {
            return;
        };
        match &bounds.bounds {
            FlowVarKind::Strong(bounds) | FlowVarKind::Weak(bounds) => {
                bounds.write().lbs = Default::default();
            }
        }
    }

    fn check_func_body_result(&mut self, body: &Expr) -> Ty {
        self.check(body)
    }

    fn function_result(ty: Ty) -> Ty {
        match ty {
            Ty::Unary(unary) if unary.op == UnaryOp::Return => unary.lhs.clone(),
            Ty::Builtin(BuiltinTy::FlowNone) => Ty::iter_union(std::iter::empty::<Ty>()),
            Ty::Union(types) => Ty::from_types(
                types
                    .iter()
                    .cloned()
                    .map(Self::function_result)
                    .collect::<Vec<_>>()
                    .into_iter(),
            ),
            ty => ty,
        }
    }

    pub(super) fn split_control_flow(ty: Ty) -> (Vec<Ty>, Option<Ty>) {
        match ty {
            Ty::Union(types) => {
                let mut controls = Vec::new();
                let mut normals = Vec::new();
                for ty in types.iter().cloned() {
                    let (mut inner_controls, normal) = Self::split_control_flow(ty);
                    controls.append(&mut inner_controls);
                    if let Some(normal) = normal {
                        normals.push(normal);
                    }
                }

                let normal = (!normals.is_empty()).then(|| Ty::from_types(normals.into_iter()));
                (controls, normal)
            }
            ty if Self::is_control_flow(&ty) => (vec![ty], None),
            ty => (vec![], Some(ty)),
        }
    }

    pub(super) fn merge_control_flow(mut controls: Vec<Ty>, normal: Option<Ty>) -> Ty {
        if let Some(normal) = normal {
            controls.push(normal);
        }
        Ty::from_types(controls.into_iter())
    }

    pub(super) fn map_normal_flow(ty: Ty, f: impl FnOnce(Ty) -> Ty) -> Ty {
        let (controls, normal) = Self::split_control_flow(ty);
        Self::merge_control_flow(controls, normal.map(f))
    }

    pub(super) fn is_control_flow(ty: &Ty) -> bool {
        matches!(
            ty,
            Ty::Builtin(BuiltinTy::Break | BuiltinTy::Continue | BuiltinTy::FlowNone)
        ) || matches!(ty, Ty::Unary(unary) if unary.op == UnaryOp::Return)
    }

    pub(super) fn instantiate_resultant(&mut self, ty: &Ty) -> Ty {
        // Recursive dispatcher patterns can re-enter generated return vars via
        // callback resultants. Keep normalization finite and leave the rest as a
        // compact deferred operand instead of expanding the same graph forever.
        let mut budget = MAX_RESULTANT_INSTANTIATION_STEPS;
        self.instantiate_resultant_(ty, 0, &mut budget)
    }

    pub(super) fn instantiate_sig_result(
        &mut self,
        sig: &Interned<SigTy>,
        args: &Interned<ArgsTy>,
        withs: Option<&Vec<Interned<SigTy>>>,
    ) -> Option<Ty> {
        if !Self::deferred_call_can_instantiate(args, withs) {
            return None;
        }

        self.with_scope(|base| {
            for (arg_recv, arg_ins) in sig.matches(args, withs) {
                if let Ty::Var(arg_recv) = arg_recv {
                    base.bind_local(arg_recv, arg_ins.clone());
                }
            }

            let body = sig.body.clone()?;
            Some(
                base.instantiate_resultant(&body)
                    .compact_deferred_resultant(),
            )
        })
    }

    pub(super) fn has_generated_var(ty: &Ty) -> bool {
        match ty {
            Ty::Var(var) => matches!(var.def.as_ref(), Decl::Generated(_)),
            Ty::Param(param) => Self::has_generated_var(&param.ty),
            Ty::Array(elem) => Self::has_generated_var(elem),
            Ty::Tuple(elems) | Ty::Union(elems) => elems.iter().any(Self::has_generated_var),
            Ty::Dict(record) => record
                .interface()
                .any(|(_, ty)| Self::has_generated_var(ty)),
            Ty::Func(sig) | Ty::Args(sig) | Ty::Pattern(sig) => {
                sig.inputs().any(Self::has_generated_var)
                    || sig.body.as_ref().is_some_and(Self::has_generated_var)
            }
            Ty::With(with) => {
                Self::has_generated_var(&with.sig)
                    || with.with.inputs().any(Self::has_generated_var)
                    || with.with.body.as_ref().is_some_and(Self::has_generated_var)
            }
            Ty::Apply(apply) => {
                Self::has_generated_var(&apply.callee)
                    || apply.args.inputs().any(Self::has_generated_var)
                    || apply
                        .args
                        .body
                        .as_ref()
                        .is_some_and(Self::has_generated_var)
            }
            Ty::Select(select) => Self::has_generated_var(&select.ty),
            Ty::Unary(unary) => Self::has_generated_var(&unary.lhs),
            Ty::Binary(binary) => {
                let [lhs, rhs] = binary.operands();
                Self::has_generated_var(lhs) || Self::has_generated_var(rhs)
            }
            Ty::If(if_ty) => {
                Self::has_generated_var(&if_ty.cond)
                    || Self::has_generated_var(&if_ty.then)
                    || Self::has_generated_var(&if_ty.else_)
            }
            Ty::Let(bounds) => bounds
                .lbs
                .iter()
                .chain(bounds.ubs.iter())
                .any(Self::has_generated_var),
            Ty::Any | Ty::Boolean(_) | Ty::Builtin(_) | Ty::Value(_) => false,
        }
    }

    pub(super) fn sig_has_generated_var(sig: &SigTy) -> bool {
        sig.inputs().any(Self::has_generated_var)
            || sig.body.as_ref().is_some_and(Self::has_generated_var)
    }

    fn deferred_call_can_instantiate(
        args: &Interned<ArgsTy>,
        withs: Option<&Vec<Interned<SigTy>>>,
    ) -> bool {
        !Self::sig_has_generated_var(args)
            && !withs
                .map(|withs| withs.iter().any(|with| Self::sig_has_generated_var(with)))
                .unwrap_or(false)
    }

    pub(super) fn is_inferring_return(&self, ty: &Ty) -> bool {
        self.inferring_func_returns.iter().any(|ret| ret == ty)
    }

    fn instantiate_resultant_(&mut self, ty: &Ty, depth: usize, budget: &mut usize) -> Ty {
        if depth >= MAX_RESULTANT_INSTANTIATION_DEPTH || *budget == 0 {
            return ty.compact_deferred_operand();
        }
        *budget -= 1;

        match ty {
            Ty::Var(var) => {
                if !self.resultant_visiting.insert(var.def.clone()) {
                    return Ty::Var(var.clone());
                }
                if let Some(local) = self.local_bind_of(var) {
                    let res = self.instantiate_resultant_(&local, depth + 1, budget);
                    self.resultant_visiting.remove(&var.def);
                    return res;
                }
                let lbs = self
                    .info
                    .vars
                    .get(&var.def)
                    .map(|bounds| bounds.bounds.bounds().read().lbs.clone());
                let Some(lbs) = lbs else {
                    self.resultant_visiting.remove(&var.def);
                    return Ty::Var(var.clone());
                };
                let mut lbs = lbs
                    .iter()
                    .take(MAX_RESULTANT_INSTANTIATION_WIDTH + 1)
                    .cloned()
                    .collect::<Vec<_>>();
                if lbs.is_empty() {
                    self.resultant_visiting.remove(&var.def);
                    return Ty::Var(var.clone());
                }
                if lbs.len() > MAX_RESULTANT_INSTANTIATION_WIDTH {
                    self.resultant_visiting.remove(&var.def);
                    return Ty::Var(var.clone());
                }

                let resultants = lbs
                    .drain(..)
                    .map(|lb| self.instantiate_resultant_(&lb, depth + 1, budget))
                    .collect::<Vec<_>>();
                self.resultant_visiting.remove(&var.def);
                Ty::from_types(resultants.into_iter())
            }
            Ty::Param(param) => Ty::Param(
                ParamTy {
                    name: param.name.clone(),
                    docs: param.docs.clone(),
                    default: param.default.clone(),
                    attrs: param.attrs,
                    ty: self.instantiate_resultant_(&param.ty, depth + 1, budget),
                }
                .into(),
            ),
            Ty::Array(elem) => {
                Ty::Array(self.instantiate_resultant_(elem, depth + 1, budget).into())
            }
            Ty::Tuple(elems) => Ty::Tuple(
                elems
                    .iter()
                    .take(MAX_RESULTANT_INSTANTIATION_WIDTH)
                    .map(|elem| self.instantiate_resultant_(elem, depth + 1, budget))
                    .collect::<Vec<_>>()
                    .into(),
            ),
            Ty::Dict(record) => Ty::Dict(
                RecordTy::new(
                    record
                        .interface()
                        .take(MAX_RESULTANT_INSTANTIATION_WIDTH)
                        .map(|(name, ty)| {
                            (
                                name.clone(),
                                self.instantiate_resultant_(ty, depth + 1, budget),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
                .into(),
            ),
            Ty::Func(sig) => Ty::Func(
                sig.as_ref()
                    .clone()
                    .with_body(
                        sig.body
                            .as_ref()
                            .map(|body| self.instantiate_resultant_(body, depth + 1, budget))
                            .unwrap_or(Ty::Any),
                    )
                    .into(),
            ),
            Ty::With(with) => Ty::With(
                SigWithTy {
                    sig: self
                        .instantiate_resultant_(&with.sig, depth + 1, budget)
                        .into(),
                    with: with.with.clone(),
                }
                .into(),
            ),
            Ty::Apply(apply) => {
                let callee = self.instantiate_resultant_(&apply.callee, depth + 1, budget);
                let args = self.instantiate_apply_args(&apply.args, depth + 1, budget);
                self.instantiate_deferred_apply(callee, args, depth + 1, budget)
            }
            Ty::Select(select) => Ty::Select(
                SelectTy::new(
                    self.instantiate_resultant_(&select.ty, depth + 1, budget)
                        .compact_deferred_operand()
                        .into(),
                    select.select.clone(),
                )
                .into(),
            ),
            Ty::Unary(unary) if unary.op == UnaryOp::TypeOf => self
                .instantiate_resultant_(&unary.lhs, depth + 1, budget)
                .type_of_result(),
            Ty::Unary(unary) => Ty::Unary(TypeUnary::new(
                unary.op,
                self.instantiate_resultant_(&unary.lhs, depth + 1, budget),
            )),
            Ty::Binary(binary) => {
                let [lhs, rhs] = binary.operands();
                let lhs = self.instantiate_resultant_(lhs, depth + 1, budget);
                let rhs = self.instantiate_resultant_(rhs, depth + 1, budget);
                self.fold_deferred_binary(binary.op, lhs, rhs)
            }
            Ty::If(if_ty) => {
                let cond = self.instantiate_resultant_(&if_ty.cond, depth + 1, budget);
                let then = self.instantiate_resultant_(&if_ty.then, depth + 1, budget);
                let else_ = self.instantiate_resultant_(&if_ty.else_, depth + 1, budget);
                match Self::known_bool(&cond) {
                    Some(true) => then,
                    Some(false) => else_,
                    None => Ty::from_types([then, else_].into_iter()),
                }
            }
            Ty::Union(types) => Ty::Union(
                types
                    .iter()
                    .take(MAX_RESULTANT_INSTANTIATION_WIDTH)
                    .map(|ty| self.instantiate_resultant_(ty, depth + 1, budget))
                    .collect::<Vec<_>>()
                    .into(),
            ),
            Ty::Let(bounds) => Ty::Let(
                TypeBounds {
                    lbs: bounds
                        .lbs
                        .iter()
                        .take(MAX_RESULTANT_INSTANTIATION_WIDTH)
                        .map(|ty| self.instantiate_resultant_(ty, depth + 1, budget))
                        .collect(),
                    ubs: bounds
                        .ubs
                        .iter()
                        .take(MAX_RESULTANT_INSTANTIATION_WIDTH)
                        .map(|ty| self.instantiate_resultant_(ty, depth + 1, budget))
                        .collect(),
                }
                .into(),
            ),
            ty => ty.clone(),
        }
    }

    fn instantiate_apply_args(
        &mut self,
        args: &Interned<ArgsTy>,
        depth: usize,
        budget: &mut usize,
    ) -> Interned<ArgsTy> {
        let mut args = args.as_ref().clone();
        args.inputs = args
            .inputs
            .iter()
            .map(|ty| self.instantiate_resultant_(ty, depth + 1, budget))
            .collect::<Vec<_>>()
            .into();
        args.body = args
            .body
            .as_ref()
            .map(|body| self.instantiate_resultant_(body, depth + 1, budget));
        args.into()
    }

    fn instantiate_deferred_apply(
        &mut self,
        callee: Ty,
        args: Interned<ArgsTy>,
        depth: usize,
        budget: &mut usize,
    ) -> Ty {
        let callee = self.resolve_deferred_select_callee(callee);
        let resultants = {
            let mut worker = ApplyTypeChecker {
                base: self,
                call_site: Span::detached(),
                allow_deferred_calls: true,
                call_raw_for_with: Some(callee.clone()),
                resultant: vec![],
            };
            callee.call(&args, true, &mut worker);
            worker.resultant
        };
        if resultants.is_empty() {
            return if self.should_preserve_unresolved_apply(&callee) {
                Ty::Apply(ApplyTy::new(callee.into(), args))
            } else {
                Self::call_resultant(resultants)
            };
        }

        let res = Self::call_resultant(resultants);
        self.instantiate_resultant_(&res, depth + 1, budget)
    }

    fn resolve_deferred_select_callee(&mut self, callee: Ty) -> Ty {
        let Ty::Select(sel) = callee else {
            return callee;
        };

        let mut worker = SelectFieldChecker {
            base: self,
            resultant: vec![],
        };
        sel.ty.select(&sel.select, true, &mut worker);
        if worker.resultant.is_empty() {
            return Ty::Select(sel);
        }

        let resultants = TypeChecker::dedup_select_resultants(worker.resultant);
        Ty::from_types(resultants.into_iter())
    }

    pub(super) fn should_preserve_unresolved_apply(&self, callee: &Ty) -> bool {
        match callee {
            Ty::Var(var) => {
                self.local_bind_of(var).is_some() || self.info.vars.contains_key(&var.def)
            }
            Ty::Apply(_) | Ty::Select(_) => true,
            Ty::Param(param) => self.should_preserve_unresolved_apply(&param.ty),
            Ty::Union(types) => types
                .iter()
                .any(|ty| self.should_preserve_unresolved_apply(ty)),
            Ty::Let(bounds) => bounds
                .lbs
                .iter()
                .chain(bounds.ubs.iter())
                .any(|ty| self.should_preserve_unresolved_apply(ty)),
            Ty::Unary(_) | Ty::Binary(_) | Ty::If(_) => true,
            Ty::With(with) => self.should_preserve_unresolved_apply(&with.sig),
            Ty::Any
            | Ty::Boolean(_)
            | Ty::Builtin(_)
            | Ty::Value(_)
            | Ty::Dict(_)
            | Ty::Array(_)
            | Ty::Tuple(_)
            | Ty::Func(_)
            | Ty::Args(_)
            | Ty::Pattern(_) => false,
        }
    }

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
        let var = if let Some(entry) = self.info.vars.get(decl) {
            entry.var.clone()
        } else {
            let name = decl.name().clone();
            let decl = decl.clone();
            let bounds = TypeVarBounds::new(
                TypeVar {
                    name: name.clone(),
                    def: decl.clone(),
                },
                DynTypeBounds::default(),
            );
            let var = bounds.var.clone();
            self.info.vars.insert(decl.clone(), bounds);

            // Check External variables
            let init = decl.file_id().and_then(|fid| {
                if fid == self.ei.fid {
                    return None;
                }

                crate::log_debug_ct!("import_ty {name} from {fid:?}");

                let ext_def_use_info = self.ctx.expr_stage_by_id(fid)?;
                let ext_def = ext_def_use_info.exports.get(&name)?;
                let source = &ext_def_use_info.source;
                let ext_func_decl = match ext_def {
                    Expr::Decl(ext_decl) if matches!(ext_decl.kind(), DefKind::Function) => {
                        Some(ext_decl.clone())
                    }
                    _ => None,
                };

                if let Some(ext_decl) = ext_func_decl.as_ref() {
                    if self.env.visiting.contains_key(&source.id()) {
                        if let Some(func) = Self::find_func_expr(&ext_def_use_info.root, ext_decl) {
                            let sig = self.check_func_sig_shallow(&func);
                            let mut bounds = DynTypeBounds::default();
                            bounds.lbs.insert_mut(sig);
                            return Some(bounds);
                        }
                    } else if let Some(ext_type_info) = type_check_for_precise_signature_(
                        self.ctx,
                        ext_def_use_info.clone(),
                        ext_decl,
                        self.env,
                    ) {
                        let ext_ty = ext_type_info.vars.get(ext_decl)?.as_type();
                        if let Some(ext_docs) = ext_type_info.var_docs.get(ext_decl) {
                            self.info
                                .var_docs
                                .insert(ext_decl.clone(), ext_docs.clone());
                        }
                        let def = ext_type_info.simplify(ext_ty, false);
                        return Some(ext_type_info.to_bounds(def));
                    }
                }

                if ext_func_decl.is_none()
                    && let Some(ext_type_info) = self.env.checked.get(&source.id()).cloned()
                {
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

                    return Some(ext_type_info.to_bounds(def));
                }

                // todo: check types in cycle
                let ext_type_info = if let Some(scheme) = self.env.visiting.get(&source.id()) {
                    scheme.clone()
                } else {
                    self.ctx.type_check_(source, self.env)
                };

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

            if let Some(init) = init
                && let Some(bounds) = self.info.vars.get_mut(&decl)
            {
                *bounds = TypeVarBounds::new(
                    TypeVar {
                        name,
                        def: decl.clone(),
                    },
                    init,
                );
            }
            var
        };

        let s = decl.span();
        if !s.is_detached() {
            // todo: record decl types
            // let should_record = matches!(root.kind(), SyntaxKind::FuncCall).then(||
            // root.span());
            // if let Some(s) = should_record {
            //     self.info.witness_at_least(s, w.clone());
            // }

            self.witness_var(s, Ty::Var(var.clone()));
        }
        var
    }

    fn find_func_expr(expr: &Expr, decl: &DeclExpr) -> Option<Interned<FuncExpr>> {
        match expr {
            Expr::Func(func) if &func.decl == decl => Some(func.clone()),
            Expr::Block(exprs) => exprs
                .iter()
                .find_map(|expr| Self::find_func_expr(expr, decl)),
            _ => None,
        }
    }

    fn var_lower_depends_on(&self, from: &DeclExpr, target: &DeclExpr) -> bool {
        fn check(
            this: &TypeChecker<'_>,
            from: &DeclExpr,
            target: &DeclExpr,
            visited: &mut FxHashSet<DeclExpr>,
        ) -> bool {
            if from == target {
                return true;
            }
            if !visited.insert(from.clone()) {
                return false;
            }
            let Some(bounds) = this.info.vars.get(from) else {
                return false;
            };
            let bounds = bounds.bounds.bounds().read();
            bounds.lbs.iter().any(|ty| {
                let Ty::Var(var) = ty else {
                    return false;
                };
                check(this, &var.def, target, visited)
            })
        }

        check(self, from, target, &mut FxHashSet::default())
    }

    fn constrain_sig_inputs(
        &mut self,
        sig: &Interned<SigTy>,
        args: &Interned<SigTy>,
        with: Option<&Vec<Interned<SigTy>>>,
    ) {
        let call_desc = (sig.clone(), args.clone(), with.cloned());
        if !self.call_cache.insert(call_desc) {
            return;
        }

        let prev_checking_deferred_body = self.checking_deferred_body;
        self.checking_deferred_body = false;
        for (arg_recv, arg_ins) in sig.matches(args, with) {
            self.constrain(arg_ins, arg_recv);
        }
        self.checking_deferred_body = prev_checking_deferred_body;
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
        static FLOW_TEXT_FONT_DICT_TYPE: LazyLock<Ty> =
            LazyLock::new(|| Ty::Dict(FLOW_TEXT_FONT_DICT.clone()));

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

                if self.var_lower_depends_on(&v.def, &w.def) {
                    return;
                }

                let lhs = Ty::Var(v.clone());

                if let Some(w_bounds) = self.info.vars.get_mut(&w.def) {
                    match &w_bounds.bounds {
                        FlowVarKind::Strong(w_bounds) | FlowVarKind::Weak(w_bounds) => {
                            w_bounds.write().lbs.insert_mut(lhs);
                        }
                    }
                }
            }
            (Ty::Var(v), rhs) => {
                crate::log_debug_ct!("constrain var {v:?} ⪯ {rhs:?}");
                let Some(w) = self.info.vars.get_mut(&v.def) else {
                    return;
                };
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
                let Some(w) = self.info.vars.get(&v.def) else {
                    return;
                };
                let bound = self.weaken_constraint(lhs, &w.bounds);
                crate::log_debug_ct!("constrain var {v:?} ⪰ {bound:?}");
                match &w.bounds {
                    FlowVarKind::Strong(v) | FlowVarKind::Weak(v) => {
                        let mut v = v.write();
                        v.lbs.insert_mut(bound);
                    }
                };
            }
            (Ty::Select(sel), rhs) => {
                // Constrain field access `base.field` by constraining `base` with a record type
                // that contains the field. This enables propagating expected types back into
                // dictionary literals, e.g. `(cjk: "")` from `fonts.cjk` used as `text(font:
                // ...)`.
                let dict = Ty::Dict(RecordTy::new(vec![(sel.select.clone(), rhs.clone())]));
                self.constrain(sel.ty.as_ref(), &dict);
            }
            (Ty::Array(lhs), Ty::Array(rhs)) => {
                self.constrain(lhs, rhs);
            }
            (Ty::Tuple(lhs), Ty::Array(rhs)) => {
                for lhs in lhs.iter() {
                    self.constrain(lhs, rhs);
                }
            }
            (Ty::Tuple(lhs), Ty::Tuple(rhs)) => {
                for (lhs, rhs) in lhs.iter().zip(rhs.iter()) {
                    self.constrain(lhs, rhs);
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
            (lhs, Ty::Builtin(BuiltinTy::TextFont)) => {
                if lhs.is_dict() {
                    self.constrain(lhs, &FLOW_TEXT_FONT_DICT_TYPE);
                }
            }
            (Ty::Builtin(BuiltinTy::TextFont), rhs) => {
                if rhs.is_dict() {
                    self.constrain(&FLOW_TEXT_FONT_DICT_TYPE, rhs);
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
            (Ty::Func(lhs), Ty::Func(rhs)) => {
                crate::log_debug_ct!("constrain func {lhs:?} ⪯ {rhs:?}");
                self.constrain_sig_inputs(lhs, rhs, None);
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
                if let Some(body) = &v.with.body {
                    self.weaken(body);
                }
            }
            Ty::Apply(v) => {
                self.weaken(&v.callee);
                for ty in v.args.inputs() {
                    self.weaken(ty);
                }
                if let Some(body) = &v.args.body {
                    self.weaken(body);
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

    fn weaken_constraint(&self, term: &Ty, kind: &FlowVarKind) -> Ty {
        if matches!(kind, FlowVarKind::Strong(_)) {
            return term.clone();
        }

        if self.checking_deferred_body {
            return term.clone();
        }

        if let Ty::Value(ins_ty) = term {
            return BuiltinTy::from_value(&ins_ty.val);
        }

        term.clone()
    }
}

struct Joiner {
    break_or_continue_or_return: bool,
    content_like: bool,
    markup_like: bool,
    definite: Ty,
    unknown: bool,
    possibles: Vec<Ty>,
    /// Conditional control-flow branches do not join with fallthrough values.
    controls: Vec<Ty>,
}
impl Joiner {
    fn new(content_like: bool) -> Self {
        Self {
            content_like,
            ..Default::default()
        }
    }

    fn new_markup(content_like: bool) -> Self {
        Self {
            content_like,
            markup_like: true,
            ..Default::default()
        }
    }

    fn finalize(mut self) -> Ty {
        crate::log_debug_ct!("join: {:?} {:?}", self.possibles, self.definite);
        let value = if self.unknown
            && matches!(self.definite, Ty::Builtin(BuiltinTy::None))
            && self.possibles.is_empty()
        {
            if self.content_like {
                Ty::Builtin(BuiltinTy::Content(None))
            } else {
                Ty::Any
            }
        } else if self.content_like
            && matches!(
                self.definite,
                Ty::Builtin(BuiltinTy::Content(_) | BuiltinTy::Space)
            )
        {
            let content = Ty::Builtin(BuiltinTy::Content(None));
            if self.possibles.is_empty() {
                content
            } else {
                self.possibles.push(content);
                Ty::from_types(self.possibles.into_iter())
            }
        } else if self.possibles.is_empty() {
            self.definite
        } else if self.possibles.len() == 1 {
            self.possibles.pop().unwrap()
        } else {
            // let mut definite = self.definite.clone();
            // for p in &self.possibles {
            //     definite = definite.join(p);
            // }

            // crate::log_debug_ct!("possibles: {:?} {:?}", self.definite, self.possibles);

            Ty::Any
        };

        if self.controls.is_empty() {
            return value;
        }

        self.controls.push(value);
        Ty::from_types(self.controls.into_iter())
    }

    fn join(&mut self, child: Ty) {
        if self.break_or_continue_or_return {
            return;
        }

        let Some(child) = self.extract_control(child) else {
            return;
        };

        if matches!(&child, Ty::Unary(unary) if unary.op == UnaryOp::Return) {
            self.definite = child;
            self.possibles.clear();
            self.break_or_continue_or_return = true;
            return;
        }
        if matches!(child, Ty::Builtin(BuiltinTy::Break | BuiltinTy::Continue)) {
            self.definite = child;
            self.possibles.clear();
            self.controls.clear();
            self.break_or_continue_or_return = true;
            return;
        }

        if !self.content_like && !self.markup_like {
            match child {
                Ty::Builtin(BuiltinTy::Space | BuiltinTy::None) => {}
                Ty::Builtin(BuiltinTy::Clause | BuiltinTy::FlowNone) => {}
                child => {
                    self.definite = child;
                    self.possibles.clear();
                    self.unknown = false;
                }
            }
            return;
        }

        if self.markup_like && !Self::is_content_piece(&child) {
            return;
        }

        if Self::is_content_piece(&child)
            && (self.content_like
                || matches!(
                    self.definite,
                    Ty::Builtin(BuiltinTy::None | BuiltinTy::Content(_) | BuiltinTy::Space)
                ))
        {
            if self.content_like {
                self.move_definite_to_possible();
            }
            self.definite = Ty::Builtin(BuiltinTy::Content(None));
            return;
        }

        match (child, &self.definite) {
            (Ty::Builtin(BuiltinTy::Space | BuiltinTy::None), _) => {}
            (Ty::Builtin(BuiltinTy::Clause | BuiltinTy::FlowNone), _) => {}
            (Ty::Any, Ty::Builtin(BuiltinTy::None)) => self.unknown = true,
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
            (Ty::Value(ins_ty), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Value(ins_ty),
            (Ty::Value(..), _) => self.definite = Ty::undef(),
            (Ty::Func(func), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Func(func),
            (Ty::Func(..), _) => self.definite = Ty::undef(),
            (Ty::Dict(dict), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Dict(dict),
            (Ty::Dict(..), _) => self.definite = Ty::undef(),
            (Ty::With(with), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::With(with),
            (Ty::With(..), _) => self.definite = Ty::undef(),
            (Ty::Args(args), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Args(args),
            (Ty::Args(..), _) => self.definite = Ty::undef(),
            (Ty::Pattern(pat), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Pattern(pat),
            (Ty::Pattern(..), _) => self.definite = Ty::undef(),
            (Ty::Select(sel), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Select(sel),
            (Ty::Select(..), _) => self.definite = Ty::undef(),
            (Ty::Apply(apply), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Apply(apply),
            (Ty::Apply(..), _) => self.definite = Ty::undef(),
            (Ty::Unary(unary), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Unary(unary),
            (Ty::Unary(..), _) => self.definite = Ty::undef(),
            (Ty::Binary(binary), Ty::Builtin(BuiltinTy::None)) => {
                self.definite = Ty::Binary(binary)
            }
            (Ty::Binary(..), _) => self.definite = Ty::undef(),
            (Ty::If(if_ty), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::If(if_ty),
            (Ty::If(..), _) => self.definite = Ty::undef(),
            (Ty::Union(types), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Union(types),
            (Ty::Union(..), _) => self.definite = Ty::undef(),
            (Ty::Let(bounds), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Let(bounds),
            (Ty::Let(..), _) => self.definite = Ty::undef(),
            (Ty::Param(param), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Param(param),
            (Ty::Param(..), _) => self.definite = Ty::undef(),
            (Ty::Boolean(b), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Boolean(b),
            (Ty::Boolean(..), _) => self.definite = Ty::undef(),
        }
    }

    fn move_definite_to_possible(&mut self) {
        if matches!(
            self.definite,
            Ty::Builtin(BuiltinTy::None | BuiltinTy::Content(_) | BuiltinTy::Space)
        ) || self.possibles.contains(&self.definite)
        {
            return;
        }

        self.possibles.push(self.definite.clone());
    }

    fn is_content_piece(ty: &Ty) -> bool {
        match ty {
            Ty::Builtin(BuiltinTy::Content(_) | BuiltinTy::Space) => true,
            Ty::Value(ins) => matches!(ins.val, Value::Content(_)),
            Ty::Param(param) => Self::is_content_piece(&param.ty),
            Ty::Union(types) => Self::is_content_union(types.iter()),
            Ty::Let(bounds) if !bounds.lbs.is_empty() => Self::is_content_union(bounds.lbs.iter()),
            Ty::Let(bounds) if bounds.ubs.len() == 1 => Self::is_content_piece(&bounds.ubs[0]),
            _ => false,
        }
    }

    fn is_content_union<'a>(types: impl Iterator<Item = &'a Ty>) -> bool {
        let mut has_content = false;
        for ty in types {
            if Self::is_content_piece(ty) {
                has_content = true;
            } else if !Self::is_none_like_piece(ty) {
                return false;
            }
        }
        has_content
    }

    fn is_none_like_piece(ty: &Ty) -> bool {
        match ty {
            Ty::Builtin(BuiltinTy::None | BuiltinTy::Clause | BuiltinTy::FlowNone) => true,
            Ty::Union(types) => types.iter().all(Self::is_none_like_piece),
            Ty::Let(bounds) => {
                let mut has_content = false;
                let mut has_none = false;
                for ty in bounds.lbs.iter() {
                    if Self::is_none_like_piece(ty) {
                        has_none = true;
                        continue;
                    }
                    if Self::is_content_piece(ty) {
                        has_content = true;
                        continue;
                    }
                    return false;
                }
                has_none && !has_content
            }
            _ => false,
        }
    }

    fn extract_control(&mut self, child: Ty) -> Option<Ty> {
        let (control, normal) = TypeChecker::split_control_flow(child);
        if control.is_empty() {
            return normal;
        }

        for ty in control {
            if !self.controls.contains(&ty) {
                self.controls.push(ty);
            }
        }

        match normal {
            None => {
                self.definite = Ty::from_types(self.controls.drain(..));
                self.possibles.clear();
                self.break_or_continue_or_return = true;
                None
            }
            Some(normal) => Some(normal),
        }
    }
}
impl Default for Joiner {
    fn default() -> Self {
        Self {
            break_or_continue_or_return: false,
            content_like: false,
            markup_like: false,
            definite: Ty::Builtin(BuiltinTy::None),
            unknown: false,
            possibles: Vec::new(),
            controls: Vec::new(),
        }
    }
}
