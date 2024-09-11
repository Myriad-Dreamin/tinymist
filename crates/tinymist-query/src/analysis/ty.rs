//! Type checking on source file

use std::{collections::HashMap, sync::Arc};

use once_cell::sync::Lazy;
use reflexo::vector::ir::DefId;
use typst::{
    foundations::Value,
    syntax::{
        ast::{self, AstNode},
        LinkedNode, Source, Span, SyntaxKind,
    },
};

use crate::analysis::{Ty, *};
use crate::{analysis::TypeScheme, ty::TypeInterface, AnalysisContext};

use super::{
    resolve_global_value, BuiltinTy, DefUseInfo, FlowVarKind, IdentRef, TypeBounds, TypeVar,
    TypeVarBounds,
};

mod apply;
mod post_check;
mod syntax;

pub(crate) use apply::*;
pub(crate) use post_check::*;

/// Type checking at the source unit level.
pub(crate) fn type_check(ctx: &mut AnalysisContext, source: Source) -> Option<Arc<TypeScheme>> {
    let mut info = TypeScheme::default();

    // Retrieve def-use information for the source.
    let def_use_info = ctx.def_use(source.clone())?;

    let mut type_checker = TypeChecker {
        ctx,
        source: source.clone(),
        def_use_info,
        info: &mut info,
        externals: HashMap::new(),
        mode: InterpretMode::Markup,
    };
    let lnk = LinkedNode::new(source.root());

    let type_check_start = std::time::Instant::now();
    type_checker.check(lnk);
    let elapsed = type_check_start.elapsed();
    log::info!("Type checking on {:?} took {elapsed:?}", source.id());

    Some(Arc::new(info))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InterpretMode {
    Markup,
    Code,
    Math,
}

struct TypeChecker<'a, 'w> {
    ctx: &'a mut AnalysisContext<'w>,
    source: Source,
    def_use_info: Arc<DefUseInfo>,

    info: &'a mut TypeScheme,
    externals: HashMap<DefId, Option<Ty>>,
    mode: InterpretMode,
}

impl<'a, 'w> TypeChecker<'a, 'w> {
    fn check(&mut self, root: LinkedNode) -> Ty {
        let should_record = matches!(root.kind(), SyntaxKind::FuncCall).then(|| root.span());
        let w = self.check_syntax(root).unwrap_or(Ty::undef());

        if let Some(s) = should_record {
            self.info.witness_at_least(s, w.clone());
        }

        w
    }

    fn get_var(&mut self, s: Span, r: IdentRef) -> Option<Ty> {
        let def_id = self
            .def_use_info
            .get_ref(&r)
            .or_else(|| Some(self.def_use_info.get_def(s.id()?, &r)?.0))?;

        // todo: false positive of clippy
        #[allow(clippy::map_entry)]
        if !self.info.vars.contains_key(&def_id) {
            let def = self.import_ty(def_id);
            let init_expr = self.init_var(def);
            self.info.vars.insert(
                def_id,
                TypeVarBounds::new(
                    TypeVar {
                        name: r.name.as_str().into(),
                        def: def_id,
                    },
                    init_expr,
                ),
            );
        }

        let var = self.info.vars.get_mut(&def_id).unwrap();
        TypeScheme::witness_(s, var.as_type(), &mut self.info.mapping);
        Some(var.as_type())
    }

    fn import_ty(&mut self, def_id: DefId) -> Option<Ty> {
        if let Some(ty) = self.externals.get(&def_id) {
            return ty.clone();
        }

        let (def_id, def_pos) = self.def_use_info.get_def_by_id(def_id)?;
        if def_id == self.source.id() {
            return None;
        }

        let source = self.ctx.source_by_id(def_id).ok()?;
        let ext_def_use_info = self.ctx.def_use(source.clone())?;
        let ext_type_info = self.ctx.type_check(source)?;
        let (ext_def_id, _) = ext_def_use_info.get_def(
            def_id,
            &IdentRef {
                name: def_pos.name.clone(),
                range: def_pos.range.clone(),
            },
        )?;
        let ext_ty = ext_type_info.vars.get(&ext_def_id)?.as_type();
        Some(ext_type_info.simplify(ext_ty, false))
    }

    fn constrain(&mut self, lhs: &Ty, rhs: &Ty) {
        static FLOW_STROKE_DICT_TYPE: Lazy<Ty> = Lazy::new(|| Ty::Dict(FLOW_STROKE_DICT.clone()));
        static FLOW_MARGIN_DICT_TYPE: Lazy<Ty> = Lazy::new(|| Ty::Dict(FLOW_MARGIN_DICT.clone()));
        static FLOW_INSET_DICT_TYPE: Lazy<Ty> = Lazy::new(|| Ty::Dict(FLOW_INSET_DICT.clone()));
        static FLOW_OUTSET_DICT_TYPE: Lazy<Ty> = Lazy::new(|| Ty::Dict(FLOW_OUTSET_DICT.clone()));
        static FLOW_RADIUS_DICT_TYPE: Lazy<Ty> = Lazy::new(|| Ty::Dict(FLOW_RADIUS_DICT.clone()));

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
                _ => Ty::Unary(TypeUnary::new(UnaryOp::ElementOf, container.into())),
            }
        } else {
            // todo: remove not element of
            Ty::Unary(TypeUnary::new(UnaryOp::NotElementOf, container.into()))
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
            Ty::Func(v) | Ty::Args(v) => {
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
