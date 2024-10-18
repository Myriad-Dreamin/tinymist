//! Type checking on source file

use tinymist_derive::BindTyCtx;

use super::{
    prelude::*, resolve_global_value, BuiltinTy, FlowVarKind, TyCtxMut, TypeBounds, TypeScheme,
    TypeVar, TypeVarBounds,
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
pub(crate) use docs::*;
pub(crate) use select::*;

/// Type checking at the source unit level.
pub(crate) fn type_check(ctx: &mut AnalysisContext, source: Source) -> Option<Arc<TypeScheme>> {
    let mut info = TypeScheme::default();

    // Retrieve def-use information for the source.
    let expr_info = ctx.expr_stage(source.clone());
    let root = expr_info.root.clone();

    let mut type_checker = TypeChecker {
        ctx,
        source: source.clone(),
        expr_info,
        info: &mut info,
    };

    let type_check_start = std::time::Instant::now();
    type_checker.check(&root);
    let elapsed = type_check_start.elapsed();
    log::info!("Type checking on {:?} took {elapsed:?}", source.id());

    Some(Arc::new(info))
}

#[derive(BindTyCtx)]
#[bind(info)]
struct TypeChecker<'a, 'w> {
    ctx: &'a mut AnalysisContext<'w>,
    source: Source,
    expr_info: Arc<ExprInfo>,

    info: &'a mut TypeScheme,
}

impl<'a, 'w> TyCtxMut for TypeChecker<'a, 'w> {
    type Snap = <TypeScheme as TyCtxMut>::Snap;

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
}

impl<'a, 'w> TypeChecker<'a, 'w> {
    fn check(&mut self, root: &Expr) -> Ty {
        // let should_record = matches!(root.kind(), SyntaxKind::FuncCall).then(||
        // root.span());
        let w = self.check_syntax(root).unwrap_or(Ty::undef());

        // if let Some(s) = should_record {
        //     self.info.witness_at_least(s, w.clone());
        // }

        w
    }

    fn copy_based_on(&mut self, fr: &TypeVarBounds, offset: u64, id: DefId) -> Ty {
        // let encoded = DefId((id.0 + 1) * 0x100_0000_0000 + offset + fr.id().0);
        // log::debug!("copy var {fr:?} as {encoded:?}");
        // let bounds = TypeVarBounds::new(
        //     TypeVar {
        //         name: fr.name().clone(),
        //         def: encoded,
        //     },
        //     fr.bounds.bounds().read().clone(),
        // );
        // let var = bounds.as_type();
        // self.info.vars.insert(encoded, bounds);
        // var
        todo!()
    }

    // fn get_var(&mut self, root: &LinkedNode<'_>, ident: ast::Ident) ->
    // Option<Interned<TypeVar>> {     let s = ident.span();
    //     let r = to_ident_ref(root, ident)?;
    //     let def_id = self.get_def_id(s, &r)?;
    //     self.get_var_by_id(s, r.name.as_ref().into(), def_id)
    // }

    fn get_var(&mut self, decl: &DeclExpr) -> Interned<TypeVar> {
        let name = decl.name().clone();
        self.get_var_by_id(decl.span(), name, decl)
    }

    fn get_var_by_id(
        &mut self,
        s: Option<Span>,
        name: Interned<str>,
        decl: &DeclExpr,
    ) -> Interned<TypeVar> {
        let var = match decl.as_ref() {
            Decl::Export { fid, name } => {
                if let Some(var) = self.info.vars.get(decl) {
                    var.var.clone()
                } else {
                    let def = self.import_ty(*fid, name);
                    let init_expr = self.init_var(def);
                    let name = name.clone();
                    let def = decl.clone();

                    let var_with_bounds = TypeVarBounds::new(TypeVar { name, def }, init_expr);
                    let var = var_with_bounds.var.clone();

                    self.info.vars.insert(decl.clone(), var_with_bounds);
                    var
                }
            }
            _ => {
                let entry = self.info.vars.entry(decl.clone()).or_insert_with(|| {
                    let name = name.clone();
                    let decl = decl.clone();
                    TypeVarBounds::new(TypeVar { name, def: decl }, TypeBounds::default())
                });

                entry.var.clone()
            }
        };
        if let Some(s) = s {
            TypeScheme::witness_(s, Ty::Var(var.clone()), &mut self.info.mapping);
        }
        var
    }

    fn import_ty(&mut self, fid: TypstFileId, name: &Interned<str>) -> Option<Ty> {
        log::debug!("import_ty {name} from {fid:?}");

        let source = self.ctx.source_by_id(fid).ok()?;
        let ext_def_use_info = self.ctx.expr_stage(source.clone());
        let ext_type_info = self.ctx.type_check(&source)?;
        let ext_def = ext_def_use_info.exports.get(name)?;

        // todo: rest expressions
        match ext_def {
            Expr::Decl(decl) => {
                let ext_ty = ext_type_info.vars.get(decl)?.as_type();
                Some(ext_type_info.simplify(ext_ty, false))
            }
            _ => None,
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
                log::debug!("constrain var {v:?} ⪯ {rhs:?}");
                let w = self.info.vars.get_mut(&v.def).unwrap();
                // strict constraint on upper bound
                let bound = rhs.clone();
                match &w.bounds {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let mut w = w.write();
                        w.ubs.push(bound);
                    }
                }
            }
            (lhs, Ty::Var(v)) => {
                let w = self.info.vars.get(&v.def).unwrap();
                let bound = self.weaken_constraint(lhs, &w.bounds);
                log::debug!("constrain var {v:?} ⪰ {bound:?}");
                match &w.bounds {
                    FlowVarKind::Strong(v) | FlowVarKind::Weak(v) => {
                        let mut v = v.write();
                        v.lbs.push(bound);
                    }
                }
            }
            (Ty::Union(v), rhs) => {
                for e in v.iter() {
                    self.constrain(e, rhs);
                }
            }
            (lhs, Ty::Union(v)) => {
                for e in v.iter() {
                    self.constrain(lhs, e);
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
                    log::debug!("constrain record item {key} {lhs:?} ⪯ {rhs:?}");
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
                log::debug!("constrain type of {lhs:?} ⪯ {rhs:?}");

                self.constrain(&lhs.lhs, rhs);
            }
            (lhs, Ty::Unary(rhs)) if rhs.op == UnaryOp::TypeOf && is_ty(lhs) => {
                log::debug!(
                    "constrain type of {lhs:?} ⪯ {rhs:?} {:?}",
                    matches!(lhs, Ty::Builtin(..))
                );
                self.constrain(lhs, &rhs.lhs);
            }
            (Ty::Value(lhs), rhs) => {
                log::debug!("constrain value {lhs:?} ⪯ {rhs:?}");
                let _ = TypeScheme::witness_at_most;
                // if !lhs.1.is_detached() {
                //     self.info.witness_at_most(lhs.1, rhs.clone());
                // }
            }
            (lhs, Ty::Value(rhs)) => {
                log::debug!("constrain value {lhs:?} ⪯ {rhs:?}");
                // if !rhs.1.is_detached() {
                //     self.info.witness_at_least(rhs.1, lhs.clone());
                // }
            }
            _ => {
                log::debug!("constrain {lhs:?} ⪯ {rhs:?}");
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

    fn init_var(&mut self, def: Option<Ty>) -> TypeBounds {
        let mut store = TypeBounds::default();

        let Some(def) = def else {
            return store;
        };

        match def {
            Ty::Var(v) => {
                let w = self.info.vars.get(&v.def).unwrap();
                match &w.bounds {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let w = w.read();
                        store.lbs.extend(w.lbs.iter().cloned());
                        store.ubs.extend(w.ubs.iter().cloned());
                    }
                }
            }
            Ty::Let(v) => {
                store.lbs.extend(v.lbs.iter().cloned());
                store.ubs.extend(v.ubs.iter().cloned());
            }
            _ => {
                store.ubs.push(def);
            }
        }

        store
    }

    fn weaken(&mut self, v: &Ty) {
        match v {
            Ty::Var(v) => {
                let w = self.info.vars.get_mut(&v.def).unwrap();
                w.weaken();
            }
            Ty::Any | Ty::Boolean(_) | Ty::Builtin(_) | Ty::Value(_) => {}
            Ty::Field(v) => {
                self.weaken(&v.field);
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

fn to_ident_ref(root: &LinkedNode, c: ast::Ident) -> Option<IdentRef> {
    Some(IdentRef {
        name: c.get().clone(),
        range: root.find(c.span())?.range(),
    })
}

struct Joiner {
    break_or_continue_or_return: bool,
    definite: Ty,
    possibles: Vec<Ty>,
}
impl Joiner {
    fn finalize(self) -> Ty {
        log::debug!("join: {:?} {:?}", self.possibles, self.definite);
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

        // log::debug!("possibles: {:?} {:?}", self.definite, self.possibles);

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
            (Ty::Var(v), _) => self.possibles.push(Ty::Var(v)),
            // todo: check possibles
            (Ty::Array(e), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Array(e),
            (Ty::Array(..), _) => self.definite = Ty::undef(),
            (Ty::Tuple(e), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Tuple(e),
            (Ty::Tuple(..), _) => self.definite = Ty::undef(),
            // todo: mystery flow none
            // todo: possible some style (auto)
            (Ty::Builtin(b), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Builtin(b),
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
            (Ty::Field(w), Ty::Builtin(BuiltinTy::None)) => self.definite = Ty::Field(w),
            (Ty::Field(..), _) => self.definite = Ty::undef(),
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
