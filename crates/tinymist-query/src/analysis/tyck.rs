//! Type checking on source file

use std::{
    collections::hash_map::Entry,
    sync::{Arc, OnceLock},
};

use rustc_hash::{FxHashMap, FxHashSet};
use tinymist_derive::BindTyCtx;

use super::{
    BuiltinTy, DynTypeBounds, FlowVarKind, SharedContext, TyCtxMut, TypeInfo, TypeVar,
    TypeVarBounds, prelude::*,
};
use crate::{
    docs::UntypedDefDocs,
    syntax::{Decl, DeclExpr, Expr, ExprInfo, UnaryOp},
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
    exprs: FxHashMap<TypstFileId, Option<ExprInfo>>,
}

/// Type checking at the source unit level.
#[typst_macros::time(span = ei.source.root().span())]
pub(crate) fn type_check(
    ctx: Arc<SharedContext>,
    ei: ExprInfo,
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
        overwritten_vars: Default::default(),
        live_input_vars: Default::default(),
        input_contract_bounds: Default::default(),
    };

    let type_check_start = tinymist_std::time::Instant::now();

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
    ei: ExprInfo,

    info: TypeInfo,
    module_exports: FxHashMap<(TypstFileId, Interned<str>), OnceLock<Option<Ty>>>,

    call_cache: FxHashSet<CallCacheDesc>,
    overwritten_vars: FxHashSet<DeclExpr>,
    // A binder remains live while the current flow value can still be the function input.
    live_input_vars: FxHashSet<DeclExpr>,
    input_contract_bounds: FxHashMap<DeclExpr, DynTypeBounds>,

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
        let mut imported_docs = None;
        let mut imported_input_bounds = vec![];
        let var = match self.info.vars.entry(decl.clone()) {
            Entry::Occupied(entry) => entry.get().var.clone(),
            Entry::Vacant(entry) => {
                let name = decl.name().clone();
                let init = Self::external_var_bounds(&self.ctx, self.env, self.ei.fid, decl, &name)
                    .map(|(bounds, docs, input_bounds)| {
                        imported_docs = docs;
                        imported_input_bounds = input_bounds;
                        bounds
                    })
                    .unwrap_or_default();
                let bounds = TypeVarBounds::new(
                    TypeVar {
                        name,
                        def: decl.clone(),
                    },
                    init,
                );
                let var = bounds.var.clone();
                entry.insert(bounds);
                var
            }
        };

        if let Some(docs) = imported_docs {
            self.info.var_docs.entry(decl.clone()).or_insert(docs);
        }
        for bounds in imported_input_bounds {
            self.info
                .vars
                .entry(bounds.var.def.clone())
                .or_insert(bounds);
        }

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

    fn external_var_bounds(
        ctx: &Arc<SharedContext>,
        env: &mut TypeEnv,
        current_fid: TypstFileId,
        decl: &DeclExpr,
        name: &Interned<str>,
    ) -> Option<(
        DynTypeBounds,
        Option<Arc<UntypedDefDocs>>,
        Vec<TypeVarBounds>,
    )> {
        let fid = decl.file_id()?;
        if fid == current_fid {
            return None;
        }

        crate::log_debug_ct!("import_ty {name} from {fid:?}");

        let ext_def_use_info = ctx.expr_stage_by_id(fid)?;
        let source = &ext_def_use_info.source;
        // todo: check types in cycle
        let ext_type_info = if let Some(scheme) = env.visiting.get(&source.id()) {
            scheme.clone()
        } else {
            ctx.clone().type_check_(source, env)
        };
        let ext_def = ext_def_use_info.exports.get(name)?;

        // todo: rest expressions
        let Expr::Decl(decl) = ext_def else {
            return None;
        };

        let ext_ty = ext_type_info.vars.get(decl)?.as_type();
        let docs = ext_type_info.var_docs.get(decl).cloned();

        let def = ext_type_info.simplify(ext_ty, true);
        let input_bounds = Self::copy_external_input_bounds(&ext_type_info, &def);
        let mut bounds = DynTypeBounds::default();
        bounds.lbs.insert_mut(def);
        Some((bounds, docs, input_bounds))
    }

    fn copy_external_input_bounds(type_info: &TypeInfo, ty: &Ty) -> Vec<TypeVarBounds> {
        let mut pending = vec![];
        let mut seen = FxHashSet::default();
        Self::collect_signature_input_binders(ty, &FxHashSet::default(), &mut seen, &mut pending);

        let mut copied = vec![];
        let mut idx = 0;
        while idx < pending.len() {
            let (binder, escaped) = pending[idx].clone();
            idx += 1;

            let Some(source) = type_info.vars.get(&binder.def) else {
                continue;
            };
            let source_is_weak = matches!(&source.bounds, FlowVarKind::Weak(_));
            let bounds = source.bounds.bounds().read().freeze();

            for bound in bounds.lbs.iter().chain(&bounds.ubs) {
                Self::collect_signature_input_binders(bound, &escaped, &mut seen, &mut pending);
            }

            let mut closer = FunctionResultantCloser {
                vars: &type_info.vars,
                params: escaped,
                visiting: FxHashSet::default(),
                visited: 0,
            };
            let bounds = closer.close_bounds(&bounds);
            let mut imported =
                TypeVarBounds::new(binder.as_ref().clone(), DynTypeBounds::from(bounds));
            if source_is_weak {
                imported.weaken();
            }
            copied.push(imported);
        }

        copied
    }

    fn collect_signature_input_binders(
        ty: &Ty,
        escaped: &FxHashSet<DeclExpr>,
        seen: &mut FxHashSet<DeclExpr>,
        binders: &mut Vec<(Interned<TypeVar>, FxHashSet<DeclExpr>)>,
    ) {
        match ty {
            Ty::Func(sig) => {
                let mut direct = vec![];
                let mut direct_seen = FxHashSet::default();
                for input in sig.inputs() {
                    Self::collect_input_binders(input, &mut direct_seen, &mut direct);
                }

                let mut scope = escaped.clone();
                for binder in direct {
                    if seen.insert(binder.def.clone()) {
                        binders.push((binder.clone(), scope.clone()));
                    }
                    scope.insert(binder.def.clone());
                }

                for input in sig.inputs() {
                    Self::collect_signature_input_binders(input, &scope, seen, binders);
                }
                if let Some(body) = &sig.body {
                    Self::collect_signature_input_binders(body, &scope, seen, binders);
                }
            }
            Ty::With(with) => {
                Self::collect_signature_input_binders(&with.sig, escaped, seen, binders);
                for input in with.with.inputs() {
                    Self::collect_signature_input_binders(input, escaped, seen, binders);
                }
                if let Some(body) = &with.with.body {
                    Self::collect_signature_input_binders(body, escaped, seen, binders);
                }
            }
            Ty::Args(sig) | Ty::Pattern(sig) => {
                for input in sig.inputs() {
                    Self::collect_signature_input_binders(input, escaped, seen, binders);
                }
                if let Some(body) = &sig.body {
                    Self::collect_signature_input_binders(body, escaped, seen, binders);
                }
            }
            Ty::Param(param) => {
                Self::collect_signature_input_binders(&param.ty, escaped, seen, binders)
            }
            Ty::Union(types) | Ty::Tuple(types) => {
                for ty in types.iter() {
                    Self::collect_signature_input_binders(ty, escaped, seen, binders);
                }
            }
            Ty::Let(bounds) => {
                for ty in bounds.lbs.iter().chain(&bounds.ubs) {
                    Self::collect_signature_input_binders(ty, escaped, seen, binders);
                }
            }
            Ty::Dict(record) => {
                for ty in record.types.iter() {
                    Self::collect_signature_input_binders(ty, escaped, seen, binders);
                }
            }
            Ty::Array(elem) => Self::collect_signature_input_binders(elem, escaped, seen, binders),
            Ty::Select(select) => {
                Self::collect_signature_input_binders(&select.ty, escaped, seen, binders)
            }
            Ty::Unary(unary) => {
                Self::collect_signature_input_binders(&unary.lhs, escaped, seen, binders)
            }
            Ty::Binary(binary) => {
                let [lhs, rhs] = binary.operands();
                Self::collect_signature_input_binders(lhs, escaped, seen, binders);
                Self::collect_signature_input_binders(rhs, escaped, seen, binders);
            }
            Ty::If(if_ty) => {
                Self::collect_signature_input_binders(&if_ty.cond, escaped, seen, binders);
                Self::collect_signature_input_binders(&if_ty.then, escaped, seen, binders);
                Self::collect_signature_input_binders(&if_ty.else_, escaped, seen, binders);
            }
            Ty::Var(_) | Ty::Any | Ty::Boolean(_) | Ty::Builtin(_) | Ty::Value(_) => {}
        }
    }

    fn collect_input_binders(
        ty: &Ty,
        seen: &mut FxHashSet<DeclExpr>,
        binders: &mut Vec<Interned<TypeVar>>,
    ) {
        match ty {
            Ty::Var(var) => {
                if seen.insert(var.def.clone()) {
                    binders.push(var.clone());
                }
            }
            Ty::Func(_) | Ty::With(_) => {}
            Ty::Param(param) => Self::collect_input_binders(&param.ty, seen, binders),
            Ty::Union(types) | Ty::Tuple(types) => {
                for ty in types.iter() {
                    Self::collect_input_binders(ty, seen, binders);
                }
            }
            Ty::Let(bounds) => {
                for ty in bounds.lbs.iter().chain(&bounds.ubs) {
                    Self::collect_input_binders(ty, seen, binders);
                }
            }
            Ty::Dict(record) => {
                for ty in record.types.iter() {
                    Self::collect_input_binders(ty, seen, binders);
                }
            }
            Ty::Array(elem) => Self::collect_input_binders(elem, seen, binders),
            Ty::Args(sig) | Ty::Pattern(sig) => {
                for input in sig.inputs() {
                    Self::collect_input_binders(input, seen, binders);
                }
            }
            Ty::Select(select) => Self::collect_input_binders(&select.ty, seen, binders),
            Ty::Unary(unary) => Self::collect_input_binders(&unary.lhs, seen, binders),
            Ty::Binary(binary) => {
                let [lhs, rhs] = binary.operands();
                Self::collect_input_binders(lhs, seen, binders);
                Self::collect_input_binders(rhs, seen, binders);
            }
            Ty::If(if_ty) => {
                Self::collect_input_binders(&if_ty.cond, seen, binders);
                Self::collect_input_binders(&if_ty.then, seen, binders);
                Self::collect_input_binders(&if_ty.else_, seen, binders);
            }
            Ty::Any | Ty::Boolean(_) | Ty::Builtin(_) | Ty::Value(_) => {}
        }
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

        let rest_bind = Self::rest_arg_bind(sig, args, with);

        for (arg_recv, arg_ins) in sig.matches(args, with) {
            if rest_bind.as_ref().is_some_and(
                |(rest_var, _)| matches!(arg_recv, Ty::Var(var) if var.def == rest_var.def),
            ) {
                continue;
            }
            if matches!(arg_recv, Ty::Var(var) if !self.info.vars.contains_key(&var.def)) {
                continue;
            }

            self.constrain_sig_input(arg_ins, arg_recv);
        }

        if let Some((rest_var, rest_ty)) = rest_bind
            && self.info.vars.contains_key(&rest_var.def)
        {
            self.constrain_sig_input(&rest_ty, &Ty::Var(rest_var));
        }
    }

    fn constrain_sig_input(&mut self, actual: &Ty, input: &Ty) {
        let Ty::Var(input_var) = input else {
            self.constrain(actual, input);
            return;
        };

        let is_external = input_var
            .def
            .file_id()
            .is_some_and(|fid| !Self::same_file_id(fid, self.ei.fid));
        if is_external {
            let contract = self.info.simplify(input.clone(), false);
            self.constrain(actual, &contract);
            return;
        }

        if matches!(input_var.def.as_ref(), Decl::Docs(..)) {
            // Rebound inputs keep call inference, but never retain a live caller flow variable.
            let input_bounds = self
                .info
                .vars
                .get(&input_var.def)
                .map(|input| input.bounds.bounds().read().freeze())
                .unwrap_or_default();
            let has_contract = !input_bounds.lbs.is_empty() || !input_bounds.ubs.is_empty();
            if has_contract {
                let contract = self.info.simplify(Ty::Let(input_bounds.into()), false);
                let actual_snapshot = self.close_lower_snapshot(actual);
                if !matches!(actual_snapshot, Ty::Any) {
                    self.constrain(&actual_snapshot, input);
                }
                self.constrain(actual, &contract);
            }
            return;
        }

        self.constrain(actual, input);
    }

    fn close_lower_snapshot(&self, ty: &Ty) -> Ty {
        let bounds = match ty {
            Ty::Var(var) => {
                let Some(bounds) = self.info.vars.get(&var.def) else {
                    return Ty::Any;
                };
                let bounds = bounds.bounds.bounds().read();
                TypeBounds {
                    lbs: bounds.lbs.iter().cloned().collect(),
                    ubs: vec![],
                }
            }
            Ty::Let(bounds) => TypeBounds {
                lbs: bounds.lbs.clone(),
                ubs: vec![],
            },
            ty => TypeBounds {
                lbs: vec![ty.clone()],
                ubs: vec![],
            },
        };
        if bounds.lbs.is_empty() {
            return Ty::Any;
        }

        let mut closer = FunctionResultantCloser {
            vars: &self.info.vars,
            params: FxHashSet::default(),
            visiting: FxHashSet::default(),
            visited: 0,
        };
        let mut bounds = closer.close_bounds(&bounds);
        if bounds.ubs.is_empty() && bounds.lbs.len() == 1 {
            return bounds.lbs.pop().unwrap();
        }
        Ty::Let(bounds.into())
    }

    fn same_file_id(left: TypstFileId, right: TypstFileId) -> bool {
        left.root() == right.root() && left.vpath() == right.vpath()
    }

    fn snapshot_function_input_bounds(&self, sig: &SigTy) -> Vec<(Interned<TypeVar>, TypeBounds)> {
        let mut seen = FxHashSet::default();
        let mut binders = vec![];
        for input in sig.inputs() {
            Self::collect_input_binders(input, &mut seen, &mut binders);
        }

        binders
            .into_iter()
            .map(|binder| {
                let bounds = self
                    .info
                    .vars
                    .get(&binder.def)
                    .map(|bounds| bounds.bounds.bounds().read().freeze())
                    .unwrap_or_default();
                (binder, bounds)
            })
            .collect()
    }

    fn close_function_resultant_type(
        &self,
        body: Ty,
        input_bounds: &[(Interned<TypeVar>, TypeBounds)],
        escaped: &FxHashSet<DeclExpr>,
    ) -> Ty {
        let mut resultant_params = escaped.clone();
        resultant_params.extend(
            input_bounds
                .iter()
                .map(|(binder, _)| &binder.def)
                .filter(|def| !self.overwritten_vars.contains(*def))
                .cloned(),
        );

        let mut closer = FunctionResultantCloser {
            vars: &self.info.vars,
            params: resultant_params,
            visiting: FxHashSet::default(),
            visited: 0,
        };

        closer.mutate(&body, true).unwrap_or(body)
    }

    fn rebind_overwritten_function_inputs(
        &mut self,
        mut sig: SigTy,
        input_bounds: Vec<(Interned<TypeVar>, TypeBounds)>,
        escaped: &FxHashSet<DeclExpr>,
    ) -> SigTy {
        let mut scope = escaped.clone();
        let mut replacements: Vec<(Interned<TypeVar>, Interned<TypeVar>)> = Vec::new();

        for (binder, mut bounds) in input_bounds {
            if !self.overwritten_vars.contains(&binder.def) {
                scope.insert(binder.def.clone());
                continue;
            }
            if let Some(input_bounds) = self.input_contract_bounds.get(&binder.def) {
                let input_bounds = input_bounds.freeze();
                bounds.lbs.extend(input_bounds.lbs);
                bounds.ubs.extend(input_bounds.ubs);
            }
            bounds.lbs.sort();
            bounds.lbs.dedup();
            bounds.ubs.sort();
            bounds.ubs.dedup();

            let mut closer = FunctionResultantCloser {
                vars: &self.info.vars,
                params: scope.clone(),
                visiting: FxHashSet::default(),
                visited: 0,
            };
            let mut bounds = closer.close_bounds(&bounds);
            bounds.lbs.sort();
            bounds.lbs.dedup();
            bounds.ubs.sort();
            bounds.ubs.dedup();

            for (original, replacement) in &replacements {
                for bound in bounds.lbs.iter_mut().chain(&mut bounds.ubs) {
                    *bound =
                        Self::replace_var(bound.clone(), original, Ty::Var(replacement.clone()));
                }
            }

            let fresh_def: DeclExpr = Decl::docs(binder.def.clone(), binder.clone()).into();
            let fresh = TypeVar {
                name: binder.name.clone(),
                def: fresh_def.clone(),
            };
            let fresh = TypeVarBounds::new(fresh, DynTypeBounds::from(bounds));
            let fresh_var = fresh.var.clone();
            self.info.vars.insert(fresh_def, fresh);
            replacements.push((binder.clone(), fresh_var));
            scope.insert(binder.def.clone());
        }

        if !replacements.is_empty() {
            let mut inputs = sig.inputs.as_ref().clone();
            for input in &mut inputs {
                for (original, replacement) in &replacements {
                    *input =
                        Self::replace_var(input.clone(), original, Ty::Var(replacement.clone()));
                }
            }
            sig.inputs = inputs.into();
        }

        sig
    }

    fn rest_arg_bind(
        sig: &Interned<SigTy>,
        args: &Interned<SigTy>,
        with: Option<&Vec<Interned<SigTy>>>,
    ) -> Option<(Interned<TypeVar>, Ty)> {
        let Ty::Var(rest_var) = sig.rest_param()? else {
            return None;
        };

        let fixed_pos = sig.positional_params().len();
        let rest_pos = with
            .into_iter()
            .flat_map(|withs| withs.iter().rev())
            .flat_map(|with| with.positional_params())
            .chain(args.positional_params())
            .skip(fixed_pos)
            .cloned()
            .collect::<Vec<_>>();

        let rest_named = args
            .named_params()
            .filter(|(name, _)| sig.named(name).is_none())
            .map(|(name, ty)| (name.clone(), ty.clone()))
            .collect::<Vec<_>>();

        let rest = args.rest_param().cloned();
        let rest_args = ArgsTy::new(rest_pos.into_iter(), rest_named, None, rest, None);

        Some((rest_var.clone(), Ty::Args(rest_args.into())))
    }

    fn collect_type_vars(
        ty: &Ty,
        vars: &mut FxHashSet<DeclExpr>,
        binders: &mut Vec<Interned<TypeVar>>,
    ) {
        match ty {
            Ty::Var(var) => {
                if vars.insert(var.def.clone()) {
                    binders.push(var.clone());
                }
            }
            Ty::Param(param) => Self::collect_type_vars(&param.ty, vars, binders),
            Ty::Union(types) | Ty::Tuple(types) => {
                for ty in types.iter() {
                    Self::collect_type_vars(ty, vars, binders);
                }
            }
            Ty::Let(bounds) => {
                for ty in bounds.lbs.iter().chain(bounds.ubs.iter()) {
                    Self::collect_type_vars(ty, vars, binders);
                }
            }
            Ty::Dict(record) => {
                for ty in record.types.iter() {
                    Self::collect_type_vars(ty, vars, binders);
                }
            }
            Ty::Array(elem) => Self::collect_type_vars(elem, vars, binders),
            Ty::Func(sig) | Ty::Args(sig) | Ty::Pattern(sig) => {
                for ty in sig.inputs() {
                    Self::collect_type_vars(ty, vars, binders);
                }
                if let Some(body) = &sig.body {
                    Self::collect_type_vars(body, vars, binders);
                }
            }
            Ty::With(with) => {
                Self::collect_type_vars(&with.sig, vars, binders);
                for ty in with.with.inputs() {
                    Self::collect_type_vars(ty, vars, binders);
                }
                if let Some(body) = &with.with.body {
                    Self::collect_type_vars(body, vars, binders);
                }
            }
            Ty::Select(sel) => Self::collect_type_vars(&sel.ty, vars, binders),
            Ty::Unary(unary) => Self::collect_type_vars(&unary.lhs, vars, binders),
            Ty::Binary(binary) => {
                let [lhs, rhs] = binary.operands();
                Self::collect_type_vars(lhs, vars, binders);
                Self::collect_type_vars(rhs, vars, binders);
            }
            Ty::If(if_ty) => {
                Self::collect_type_vars(&if_ty.cond, vars, binders);
                Self::collect_type_vars(&if_ty.then, vars, binders);
                Self::collect_type_vars(&if_ty.else_, vars, binders);
            }
            Ty::Any | Ty::Boolean(_) | Ty::Builtin(_) | Ty::Value(_) => {}
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
        static FLOW_TEXT_FONT_DICT_TYPE: LazyLock<Ty> =
            LazyLock::new(|| Ty::Dict(FLOW_TEXT_FONT_DICT.clone()));

        fn type_value_instance(ty: &Ty) -> Option<Ty> {
            match ty {
                Ty::Builtin(ty @ BuiltinTy::Type(..)) => Some(Ty::Builtin(ty.clone())),
                Ty::Value(val) => match val.val {
                    Value::Type(ty) => Some(Ty::Builtin(BuiltinTy::Type(ty))),
                    _ => None,
                },
                _ => None,
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
                let Some(rhs) = self.info.vars.get(&w.def) else {
                    return;
                };
                match &rhs.bounds {
                    FlowVarKind::Strong(bounds) | FlowVarKind::Weak(bounds) => {
                        bounds.write().lbs.insert_mut(Ty::Var(v.clone()));
                    }
                }
                self.record_input_lower_bound(&w.def, Ty::Var(v.clone()));
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
                        w.ubs.insert_mut(bound.clone());
                    }
                }
                self.record_input_upper_bound(&v.def, bound);
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
                        v.lbs.insert_mut(bound.clone());
                    }
                };
                self.record_input_lower_bound(&v.def, bound);
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
                self.constrain_tuple_positions(lhs, rhs.iter());
            }
            (Ty::Tuple(lhs), Ty::Pattern(rhs)) => {
                self.constrain_tuple_positions(lhs, rhs.positional_params());
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
            (Ty::Unary(lhs), rhs) if lhs.op == UnaryOp::TypeOf => {
                if let Some(rhs) = type_value_instance(rhs) {
                    crate::log_debug_ct!("constrain type of {lhs:?} ⪯ {rhs:?}");
                    self.constrain(&lhs.lhs, &rhs);
                }
            }
            (lhs, Ty::Unary(rhs)) if rhs.op == UnaryOp::TypeOf => {
                if let Some(lhs) = type_value_instance(lhs) {
                    crate::log_debug_ct!(
                        "constrain type of {lhs:?} ⪯ {rhs:?} {:?}",
                        matches!(lhs, Ty::Builtin(..)),
                    );
                    self.constrain(&lhs, &rhs.lhs);
                }
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

    fn constrain_tuple_positions<'a>(
        &mut self,
        lhs: &[Ty],
        rhs: impl ExactSizeIterator<Item = &'a Ty>,
    ) {
        for (idx, rhs) in rhs.enumerate() {
            let mut any = false;
            for lhs in self.tuple_pos_candidates(lhs, idx) {
                any = true;
                self.constrain(&lhs, rhs);
            }

            if !any && let Some(spread) = self.tuple_open_spread(lhs, idx) {
                self.constrain(spread, &Ty::Array(rhs.clone().into()));
            }
        }
    }

    fn record_input_lower_bound(&mut self, def: &DeclExpr, bound: Ty) {
        if self.live_input_vars.contains(def) {
            self.input_contract_bounds
                .entry(def.clone())
                .or_default()
                .lbs
                .insert_mut(bound);
        }
    }

    fn record_input_upper_bound(&mut self, def: &DeclExpr, bound: Ty) {
        // `Any` is the identity of an upper-bound intersection and carries no contract fact.
        if !matches!(bound, Ty::Any) && self.live_input_vars.contains(def) {
            self.input_contract_bounds
                .entry(def.clone())
                .or_default()
                .ubs
                .insert_mut(bound);
        }
    }

    fn tuple_pos_candidates(&self, elems: &[Ty], idx: usize) -> Vec<Ty> {
        let mut pos = 0;
        let mut candidates = vec![];

        for elem in elems {
            if let Some(spread) = Self::spread_operand(elem) {
                let spread_idx = idx.saturating_sub(pos);
                self.collect_spread_pos_candidates(spread, spread_idx, &mut candidates);
                if !candidates.is_empty() {
                    return candidates;
                }

                if let Some(len) = self.fixed_spread_len(spread) {
                    pos += len;
                    continue;
                }

                if idx >= pos {
                    return candidates;
                }
            } else if pos == idx {
                candidates.push(elem.clone());
                return candidates;
            } else {
                pos += 1;
            }
        }

        candidates
    }

    fn tuple_open_spread<'a>(&self, elems: &'a [Ty], idx: usize) -> Option<&'a Ty> {
        let mut pos = 0;

        for elem in elems {
            if let Some(spread) = Self::spread_operand(elem) {
                if idx >= pos {
                    if let Some(len) = self.fixed_spread_len(spread) {
                        if idx < pos + len {
                            return Some(spread);
                        }
                        pos += len;
                        continue;
                    }
                    return Some(spread);
                }
            } else if pos == idx {
                return None;
            } else {
                pos += 1;
            }
        }

        None
    }

    fn spread_operand(ty: &Ty) -> Option<&Ty> {
        let Ty::Unary(unary) = ty else {
            return None;
        };
        (unary.op == UnaryOp::Spread).then_some(&unary.lhs)
    }

    fn fixed_spread_len(&self, ty: &Ty) -> Option<usize> {
        match ty {
            Ty::Tuple(elems) => Some(elems.len()),
            Ty::Args(args) if args.rest_param().is_none() => Some(args.positional_params().len()),
            Ty::Var(var) => {
                let bounds = self.info.vars.get(&var.def)?;
                let lbs = bounds.bounds.bounds().read().lbs.clone();
                let mut len = None;
                for lb in lbs.iter() {
                    let next = self.fixed_spread_len(lb)?;
                    if len.is_some_and(|prev| prev != next) {
                        return None;
                    }
                    len = Some(next);
                }
                len
            }
            Ty::Let(bounds) => {
                let mut len = None;
                for lb in bounds.lbs.iter() {
                    let next = self.fixed_spread_len(lb)?;
                    if len.is_some_and(|prev| prev != next) {
                        return None;
                    }
                    len = Some(next);
                }
                len
            }
            _ => None,
        }
    }

    fn collect_spread_pos_candidates(&self, ty: &Ty, idx: usize, candidates: &mut Vec<Ty>) {
        match ty {
            Ty::Array(elem) => candidates.push(elem.as_ref().clone()),
            Ty::Tuple(elems) => {
                if let Some(elem) = elems.get(idx) {
                    candidates.push(elem.clone());
                }
            }
            Ty::Args(args) => {
                if let Some(elem) = args.pos_or_rest(idx) {
                    candidates.push(elem);
                }
            }
            Ty::Var(var) => {
                if let Some(bounds) = self.info.vars.get(&var.def) {
                    let lbs = bounds.bounds.bounds().read().lbs.clone();
                    for lb in lbs.iter() {
                        self.collect_spread_pos_candidates(lb, idx, candidates);
                    }
                }
            }
            Ty::Let(bounds) => {
                for lb in bounds.lbs.iter() {
                    self.collect_spread_pos_candidates(lb, idx, candidates);
                }
            }
            _ => {}
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

    fn constrain_assignment(&mut self, lhs: &Ty, rhs: &Ty) {
        match lhs {
            Ty::Var(var) => {
                if !self.assign_var(var, rhs) {
                    self.possible_ever_be(lhs, rhs);
                }
            }
            Ty::Tuple(_) | Ty::Pattern(_) => self.constrain(rhs, lhs),
            Ty::Union(types) => {
                for lhs in types.iter() {
                    self.constrain_assignment(lhs, rhs);
                }
            }
            Ty::Let(bounds) => {
                for lhs in bounds.lbs.iter().chain(bounds.ubs.iter()) {
                    self.constrain_assignment(lhs, rhs);
                }
            }
            _ => self.possible_ever_be(lhs, rhs),
        }

        let mut vars = FxHashSet::default();
        let mut binders = vec![];
        Self::collect_type_vars(lhs, &mut vars, &mut binders);
        for binder in binders {
            self.live_input_vars.remove(&binder.def);
        }
    }

    fn assign_var(&mut self, var: &Interned<TypeVar>, rhs: &Ty) -> bool {
        if !Self::assignment_rhs_overwrites(rhs) {
            return false;
        }

        let rhs_mentions_var = match Self::type_contains_var(rhs, &var.def) {
            Some(rhs_mentions_var) => rhs_mentions_var,
            None => return false,
        };

        let rhs = if rhs_mentions_var {
            let mut snapshot = self.shallow_lower_bound(Ty::Var(var.clone()));
            if !matches!(Self::type_contains_var(&snapshot, &var.def), Some(false)) {
                snapshot = Ty::Any;
            }

            let rhs = Self::replace_var(rhs.clone(), var, snapshot);
            if !matches!(Self::type_contains_var(&rhs, &var.def), Some(false)) {
                return false;
            }
            rhs
        } else {
            rhs.clone()
        };
        let rhs = self.shallow_lower_bound(rhs);
        // Preserve the input contract before the body-flow variable is overwritten.
        if self.live_input_vars.contains(&var.def)
            && let Some(bounds) = self.info.vars.get(&var.def)
        {
            let bounds = bounds.bounds.bounds().read().freeze();
            let input = self
                .input_contract_bounds
                .entry(var.def.clone())
                .or_default();
            for bound in bounds.lbs {
                input.lbs.insert_mut(bound);
            }
            for bound in bounds.ubs {
                input.ubs.insert_mut(bound);
            }
        }
        let Some(bounds) = self.info.vars.get_mut(&var.def) else {
            return false;
        };
        self.overwritten_vars.insert(var.def.clone());
        let mut bounds = bounds.bounds.bounds().write();
        bounds.lbs = [rhs].into_iter().collect();
        true
    }

    fn assignment_rhs_overwrites(rhs: &Ty) -> bool {
        !matches!(rhs, Ty::Any | Ty::Select(_) | Ty::Binary(_))
    }

    fn replace_var(ty: Ty, var: &Interned<TypeVar>, with: Ty) -> Ty {
        let mut replacer = VarReplacer {
            def: var.def.clone(),
            with,
        };
        ty.mutate(true, &mut replacer).unwrap_or(ty)
    }

    fn type_contains_var(ty: &Ty, def: &DeclExpr) -> Option<bool> {
        const NODE_BUDGET: usize = 4096;

        let mut stack = vec![ty];
        let mut visited = 0usize;
        while let Some(ty) = stack.pop() {
            visited += 1;
            if visited > NODE_BUDGET {
                return None;
            }

            match ty {
                Ty::Var(var) if var.def == *def => return Some(true),
                Ty::Param(param) => stack.push(&param.ty),
                Ty::Union(types) | Ty::Tuple(types) => stack.extend(types.iter()),
                Ty::Let(bounds) => {
                    stack.extend(bounds.lbs.iter());
                    stack.extend(bounds.ubs.iter());
                }
                Ty::Dict(record) => stack.extend(record.types.iter()),
                Ty::Array(elem) => stack.push(elem),
                Ty::Func(sig) | Ty::Args(sig) | Ty::Pattern(sig) => {
                    stack.extend(sig.inputs());
                    if let Some(body) = &sig.body {
                        stack.push(body);
                    }
                }
                Ty::With(with) => {
                    stack.push(&with.sig);
                    stack.extend(with.with.inputs());
                    if let Some(body) = &with.with.body {
                        stack.push(body);
                    }
                }
                Ty::Select(sel) => stack.push(&sel.ty),
                Ty::Unary(unary) => stack.push(&unary.lhs),
                Ty::Binary(binary) => {
                    let [lhs, rhs] = binary.operands();
                    stack.push(lhs);
                    stack.push(rhs);
                }
                Ty::If(if_ty) => {
                    stack.push(&if_ty.cond);
                    stack.push(&if_ty.then);
                    stack.push(&if_ty.else_);
                }
                Ty::Var(_) | Ty::Any | Ty::Boolean(_) | Ty::Builtin(_) | Ty::Value(_) => {}
            }
        }

        Some(false)
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

    fn weaken_constraint(&self, term: &Ty, kind: &FlowVarKind) -> Ty {
        if matches!(kind, FlowVarKind::Strong(_)) {
            return term.clone();
        }

        if let Ty::Value(ins_ty) = term {
            return BuiltinTy::from_value(&ins_ty.val);
        }

        term.clone()
    }
}

struct ControlSplit {
    normal: Option<Ty>,
    returns: Vec<Ty>,
    terminal: bool,
}

struct FunctionResultantCloser<'a> {
    vars: &'a FxHashMap<DeclExpr, TypeVarBounds>,
    params: FxHashSet<DeclExpr>,
    visiting: FxHashSet<DeclExpr>,
    visited: usize,
}

impl FunctionResultantCloser<'_> {
    const NODE_BUDGET: usize = 4096;

    fn close_scoped_sig(&mut self, sig: &SigTy, pol: bool) -> Option<SigTy> {
        let mut seen = FxHashSet::default();
        let mut binders = vec![];
        for input in sig.inputs() {
            TypeChecker::collect_input_binders(input, &mut seen, &mut binders);
        }
        let inserted = binders
            .into_iter()
            .filter_map(|binder| {
                self.params
                    .insert(binder.def.clone())
                    .then_some(binder.def.clone())
            })
            .collect::<Vec<_>>();

        let inputs = self.mutate_vec(&sig.inputs, pol);
        let body = self.mutate_option(sig.body.as_ref(), pol);

        for def in inserted {
            self.params.remove(&def);
        }
        if inputs.is_none() && body.is_none() {
            return None;
        }

        let mut sig = sig.clone();
        if let Some(inputs) = inputs {
            sig.inputs = inputs;
        }
        if let Some(body) = body {
            sig.body = body;
        }
        Some(sig)
    }

    fn lower_bounds_of(&mut self, var: &Interned<TypeVar>) -> Option<Ty> {
        self.visited += 1;
        if self.visited > Self::NODE_BUDGET {
            return Some(Ty::Any);
        }

        if self.params.contains(&var.def) {
            return None;
        }
        if !self.visiting.insert(var.def.clone()) {
            return Some(Ty::Any);
        }

        let Some(bounds) = self.vars.get(&var.def) else {
            self.visiting.remove(&var.def);
            return Some(Ty::Any);
        };
        let bounds = bounds.bounds.bounds().read().freeze();
        if bounds.lbs.is_empty() && bounds.ubs.is_empty() {
            self.visiting.remove(&var.def);
            return Some(Ty::Any);
        }

        let bounds = self.close_bounds(&bounds);
        self.visiting.remove(&var.def);
        Some(Ty::Let(Interned::new(bounds)))
    }

    fn close_bounds(&mut self, bounds: &TypeBounds) -> TypeBounds {
        let lbs = bounds
            .lbs
            .iter()
            .map(|bound| self.mutate(bound, false).unwrap_or_else(|| bound.clone()))
            .collect();
        let ubs = bounds
            .ubs
            .iter()
            .map(|bound| self.mutate(bound, true).unwrap_or_else(|| bound.clone()))
            .collect();
        TypeBounds { lbs, ubs }
    }
}

impl TyMutator for FunctionResultantCloser<'_> {
    fn mutate(&mut self, ty: &Ty, pol: bool) -> Option<Ty> {
        match ty {
            Ty::Var(var) => self.lower_bounds_of(var),
            Ty::Let(bounds) => Some(Ty::Let(Interned::new(self.close_bounds(bounds)))),
            Ty::Func(sig) => self
                .close_scoped_sig(sig, pol)
                .map(|sig| Ty::Func(Interned::new(sig))),
            Ty::Pattern(sig) => self
                .close_scoped_sig(sig, pol)
                .map(|sig| Ty::Pattern(Interned::new(sig))),
            _ => self.mutate_rec(ty, pol),
        }
    }
}

struct Joiner {
    break_or_continue_or_return: bool,
    definite: Ty,
    possibles: Vec<Ty>,
    returns: Vec<Ty>,
}
impl Joiner {
    fn finalize(self) -> Ty {
        crate::log_debug_ct!(
            "join: {:?} {:?} returns {:?}",
            self.possibles,
            self.definite,
            self.returns
        );

        let normal = Self::finalize_normal(self.definite, self.possibles);
        if self.returns.is_empty() {
            if self.break_or_continue_or_return {
                return Ty::Builtin(BuiltinTy::Never);
            }
            return normal;
        }

        let returned = Self::finalize_types(self.returns);
        if self.break_or_continue_or_return {
            return Ty::Unary(TypeUnary::new(UnaryOp::Return, returned));
        }

        if Self::is_none_like(&normal) {
            return Ty::Any;
        }
        if normal == returned {
            return normal;
        }

        Ty::from_types([normal, returned].into_iter())
    }

    fn finalize_normal(definite: Ty, possibles: Vec<Ty>) -> Ty {
        if possibles.is_empty() {
            return definite;
        }
        if possibles.len() == 1 {
            return possibles.into_iter().next().unwrap();
        }

        // let mut definite = definite.clone();
        // for p in &possibles {
        //     definite = definite.join(p);
        // }

        // crate::log_debug_ct!("possibles: {:?} {:?}", definite, possibles);

        Ty::Any
    }

    fn finalize_types(types: Vec<Ty>) -> Ty {
        if types.len() == 1 {
            return types.into_iter().next().unwrap();
        }

        Ty::from_types(types.into_iter())
    }

    fn is_none_like(ty: &Ty) -> bool {
        matches!(
            ty,
            Ty::Builtin(
                BuiltinTy::Space | BuiltinTy::None | BuiltinTy::Clause | BuiltinTy::FlowNone
            )
        )
    }

    fn split_control(child: Ty) -> ControlSplit {
        match child {
            Ty::Unary(unary) if unary.op == UnaryOp::Return => ControlSplit {
                normal: None,
                returns: vec![unary.lhs.clone()],
                terminal: true,
            },
            Ty::Builtin(BuiltinTy::Break | BuiltinTy::Continue | BuiltinTy::Never) => {
                ControlSplit {
                    normal: None,
                    returns: vec![],
                    terminal: true,
                }
            }
            Ty::If(if_ty) => {
                let then = Self::split_control(if_ty.then.as_ref().clone());
                let else_ = Self::split_control(if_ty.else_.as_ref().clone());

                let mut returns = then.returns;
                returns.extend(else_.returns);

                let then_normal = then.normal.filter(|ty| !Self::is_none_like(ty));
                let else_normal = else_.normal.filter(|ty| !Self::is_none_like(ty));
                let normal = if then_normal.is_none() && else_normal.is_none() {
                    None
                } else {
                    Some(Ty::If(IfTy::new(
                        if_ty.cond.clone(),
                        then_normal.unwrap_or(Ty::Builtin(BuiltinTy::None)).into(),
                        else_normal.unwrap_or(Ty::Builtin(BuiltinTy::None)).into(),
                    )))
                };

                ControlSplit {
                    normal,
                    returns,
                    terminal: then.terminal && else_.terminal,
                }
            }
            normal if Self::is_none_like(&normal) => ControlSplit {
                normal: None,
                returns: vec![],
                terminal: false,
            },
            normal => ControlSplit {
                normal: Some(normal),
                returns: vec![],
                terminal: false,
            },
        }
    }

    fn join(&mut self, child: Ty) {
        if self.break_or_continue_or_return {
            return;
        }

        let ControlSplit {
            normal,
            returns,
            terminal,
        } = Self::split_control(child);

        self.returns.extend(returns);
        if let Some(normal) = normal {
            self.join_normal(normal);
        }
        if terminal {
            self.break_or_continue_or_return = true;
        }
    }

    fn join_normal(&mut self, child: Ty) {
        if matches!(self.definite, Ty::Any) && !matches!(child, Ty::Any) {
            self.definite = Ty::Builtin(BuiltinTy::None);
        }

        match (child, &self.definite) {
            (Ty::Builtin(BuiltinTy::Space | BuiltinTy::None), _) => {}
            (Ty::Builtin(BuiltinTy::Clause | BuiltinTy::FlowNone), _) => {}
            (Ty::Any, _) => self.definite = Ty::Any,
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
}
impl Default for Joiner {
    fn default() -> Self {
        Self {
            break_or_continue_or_return: false,
            definite: Ty::Builtin(BuiltinTy::None),
            possibles: Vec::new(),
            returns: Vec::new(),
        }
    }
}

struct VarReplacer {
    def: DeclExpr,
    with: Ty,
}

impl TyMutator for VarReplacer {
    fn mutate(&mut self, ty: &Ty, pol: bool) -> Option<Ty> {
        if let Ty::Var(var) = ty
            && var.def == self.def
        {
            return Some(self.with.clone());
        }

        self.mutate_rec(ty, pol)
    }
}
