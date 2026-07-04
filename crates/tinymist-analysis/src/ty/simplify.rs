#![allow(unused)]

use ecow::{EcoString, EcoVec};

use typst::foundations::Value;

use crate::{
    syntax::{BinaryOp, DeclExpr, UnaryOp},
    ty::prelude::*,
};

/// A compact type.
#[derive(Default)]
struct CompactTy {
    equiv_vars: HashSet<DefId>,
    primitives: HashSet<Ty>,
    recursives: HashMap<DefId, CompactTy>,
    signatures: Vec<Interned<SigTy>>,

    is_final: bool,
}

impl TypeInfo {
    /// Simplifies (canonicalizes) the given type with the given type scheme.
    pub fn simplify(&self, ty: Ty, principal: bool) -> Ty {
        let mut cache = self.cano_cache.lock();
        let cache = &mut *cache;

        cache.transform_cache.clear();
        cache.cano_local_cache.clear();
        cache.positives.clear();
        cache.negatives.clear();

        let mut worker = TypeSimplifier {
            principal,
            vars: &self.vars,
            doc_annotated_vars: &self.doc_annotated_vars,
            cano_cache: &mut cache.cano_cache,
            transform_cache: &mut cache.transform_cache,
            cano_local_cache: &mut cache.cano_local_cache,

            positives: &mut cache.positives,
            negatives: &mut cache.negatives,
        };

        worker.simplify(ty, principal)
    }
}

/// A simplifier to simplify a type.
struct TypeSimplifier<'a, 'b> {
    principal: bool,

    vars: &'a FxHashMap<DeclExpr, TypeVarBounds>,
    doc_annotated_vars: &'a FxHashSet<DeclExpr>,

    cano_cache: &'b mut FxHashMap<(Ty, bool), Ty>,
    transform_cache: &'b mut FxHashMap<(Ty, bool), Ty>,
    cano_local_cache: &'b mut FxHashMap<(DeclExpr, bool), Ty>,
    negatives: &'b mut FxHashSet<DeclExpr>,
    positives: &'b mut FxHashSet<DeclExpr>,
}

impl TypeSimplifier<'_, '_> {
    /// Simplifies the given type.
    fn simplify(&mut self, ty: Ty, principal: bool) -> Ty {
        if let Some(cano) = self.cano_cache.get(&(ty.clone(), principal)) {
            return cano.clone();
        }

        self.analyze(&ty, true);
        let cano = self.transform(&ty, true);
        self.cano_cache.insert((ty, principal), cano.clone());
        cano
    }

    /// Analyzes the given type.
    fn analyze(&mut self, ty: &Ty, pol: bool) {
        match ty {
            Ty::Var(var) => {
                let Some(w) = self.vars.get(&var.def) else {
                    return;
                };
                match &w.bounds {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let bounds = w.read();
                        let inserted = if pol {
                            self.positives.insert(var.def.clone())
                        } else {
                            self.negatives.insert(var.def.clone())
                        };
                        if !inserted {
                            return;
                        }

                        if pol {
                            for lb in bounds.lbs.iter() {
                                self.analyze(lb, pol);
                            }
                        } else {
                            for ub in bounds.ubs.iter() {
                                self.analyze(ub, pol);
                            }
                        }
                    }
                }
            }
            Ty::Func(func) => {
                for input_ty in func.inputs() {
                    self.analyze(input_ty, !pol);
                }
                if let Some(ret_ty) = &func.body {
                    self.analyze(ret_ty, pol);
                }
            }
            Ty::Dict(record) => {
                for member in record.types.iter() {
                    self.analyze(member, pol);
                }
            }
            Ty::Tuple(elems) => {
                for elem in elems.iter() {
                    self.analyze(elem, pol);
                }
            }
            Ty::Array(arr) => {
                self.analyze(arr, pol);
            }
            Ty::With(with) => {
                self.analyze(&with.sig, pol);
                for input in with.with.inputs() {
                    self.analyze(input, pol);
                }
                if let Some(body) = &with.with.body {
                    self.analyze(body, pol);
                }
            }
            Ty::Apply(apply) => {
                self.analyze(&apply.callee, pol);
                for input in apply.args.inputs() {
                    self.analyze(input, pol);
                }
                if let Some(body) = &apply.args.body {
                    self.analyze(body, pol);
                }
            }
            Ty::Args(args) => {
                for input in args.inputs() {
                    self.analyze(input, pol);
                }
            }
            Ty::Pattern(pat) => {
                for input in pat.inputs() {
                    self.analyze(input, pol);
                }
            }
            Ty::Unary(unary) if unary.op == UnaryOp::TypeOf => {}
            Ty::Unary(unary) => self.analyze(&unary.lhs, pol),
            Ty::Binary(binary) => {
                let [lhs, rhs] = binary.operands();
                self.analyze(lhs, pol);
                self.analyze(rhs, pol);
            }
            Ty::If(if_expr) => {
                self.analyze(&if_expr.cond, pol);
                self.analyze(&if_expr.then, pol);
                self.analyze(&if_expr.else_, pol);
            }
            Ty::Union(types) => {
                for ty in types.iter() {
                    self.analyze(ty, pol);
                }
            }
            Ty::Select(..) => {}
            Ty::Let(bounds) => {
                for lb in bounds.lbs.iter() {
                    self.analyze(lb, !pol);
                }
                for ub in bounds.ubs.iter() {
                    self.analyze(ub, pol);
                }
            }
            Ty::Param(param) => {
                self.analyze(&param.ty, pol);
            }
            Ty::Value(_v) => {}
            Ty::Any => {}
            Ty::Boolean(_) => {}
            Ty::Builtin(_) => {}
        }
    }

    /// Transforms the given type.
    fn transform(&mut self, ty: &Ty, pol: bool) -> Ty {
        if let Some(cano) = self.transform_cache.get(&(ty.clone(), pol)) {
            return cano.clone();
        }

        let cano = match ty {
            Ty::Let(bounds) => self.transform_let(bounds.lbs.iter(), bounds.ubs.iter(), None, pol),
            Ty::Var(var) => {
                if let Some(cano) = self
                    .cano_local_cache
                    .get(&(var.def.clone(), self.principal))
                {
                    return cano.clone();
                }
                let Some(var_bounds) = self.vars.get(&var.def) else {
                    return Ty::Var(var.clone());
                };
                // todo: avoid cycle
                self.cano_local_cache
                    .insert((var.def.clone(), self.principal), Ty::Any);

                let res = match &var_bounds.bounds {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let w = w.read();

                        self.transform_let(w.lbs.iter(), w.ubs.iter(), Some(&var.def), pol)
                    }
                };

                self.cano_local_cache
                    .insert((var.def.clone(), self.principal), res.clone());

                res
            }
            Ty::Func(func) => Ty::Func(self.transform_sig(func, pol)),
            Ty::Dict(record) => {
                let mut mutated = record.as_ref().clone();
                mutated.types = self.transform_seq(&mutated.types, pol);

                Ty::Dict(mutated.into())
            }
            Ty::Tuple(tup) => Ty::Tuple(self.transform_seq(tup, pol)),
            Ty::Array(arr) => Ty::Array(self.transform(arr, pol).into()),
            Ty::With(with) => {
                let sig = self.transform(&with.sig, pol).into();
                // Negate the pol to make correct covariance
                let mutated = self.transform_sig(&with.with, !pol);

                Ty::With(SigWithTy::new(sig, mutated))
            }
            Ty::Apply(apply) => Ty::Apply(ApplyTy::new(
                self.transform_deferred_operand(&apply.callee, pol, 0)
                    .into(),
                self.transform_apply_args(&apply.args, pol),
            )),
            // Negate the pol to make correct covariance
            // todo: negate?
            Ty::Args(args) => Ty::Args(self.transform_sig(args, !pol)),
            Ty::Pattern(pat) => Ty::Pattern(self.transform_sig(pat, !pol)),
            Ty::Unary(unary) if unary.op == UnaryOp::TypeOf => unary.lhs.type_of_result(),
            Ty::Unary(unary) => {
                Ty::Unary(TypeUnary::new(unary.op, self.transform(&unary.lhs, pol)))
            }
            Ty::Binary(binary) => {
                let [lhs, rhs] = binary.operands();
                let lhs = self.transform(lhs, pol);
                let rhs = self.transform(rhs, pol);

                Self::fold_binary(binary.op, lhs, rhs)
            }
            Ty::If(if_ty) => {
                let cond = self.transform(&if_ty.cond, pol);
                let then = self.transform(&if_ty.then, pol);
                let else_ = self.transform(&if_ty.else_, pol);
                match Self::known_bool(&cond) {
                    Some(true) => then,
                    Some(false) => else_,
                    None => Ty::from_types([then, else_].into_iter()),
                }
            }
            Ty::Union(types) => {
                let seq = types.iter().map(|ty| self.transform(ty, pol));
                let seq_no_any = seq.filter(|ty| !matches!(ty, Ty::Any));
                let seq = seq_no_any.collect::<Vec<_>>();
                Ty::from_types(seq.into_iter())
            }
            Ty::Param(param) => {
                let mut param = param.as_ref().clone();
                param.ty = self.transform(&param.ty, pol);

                Ty::Param(param.into())
            }
            Ty::Select(sel) => {
                let mut sel = sel.as_ref().clone();
                sel.ty = self.transform_deferred_operand(&sel.ty, pol, 0).into();

                Ty::Select(sel.into())
            }

            Ty::Value(ins_ty) => Ty::Value(ins_ty.clone()),
            Ty::Any => Ty::Any,
            Ty::Boolean(truthiness) => Ty::Boolean(*truthiness),
            Ty::Builtin(ty) => Ty::Builtin(ty.clone()),
        };

        self.transform_cache.insert((ty.clone(), pol), cano.clone());
        cano
    }

    /// Transforms a deferred operation operand with a small structural budget.
    fn transform_deferred_operand(&mut self, ty: &Ty, pol: bool, depth: usize) -> Ty {
        if depth >= 6 {
            return Ty::Any;
        }

        match ty {
            Ty::Any | Ty::Boolean(_) | Ty::Builtin(_) | Ty::Value(_) => ty.clone(),
            Ty::Param(param) => self.transform_deferred_operand(&param.ty, pol, depth + 1),
            Ty::Var(var) => {
                let Some(bounds) = self.vars.get(&var.def) else {
                    return ty.clone();
                };
                let bounds = bounds.bounds.bounds().read();
                let target = {
                    let mut lbs = bounds.lbs.iter();
                    let first = lbs.next().cloned();
                    first.filter(|_| lbs.next().is_none())
                };
                drop(bounds);
                target
                    .as_ref()
                    .map(|ty| self.transform_deferred_operand(ty, pol, depth + 1))
                    .unwrap_or_else(|| ty.clone())
            }
            Ty::Let(bounds) if bounds.lbs.len() == 1 => {
                self.transform_deferred_operand(&bounds.lbs[0], pol, depth + 1)
            }
            Ty::Let(bounds) if bounds.lbs.is_empty() && bounds.ubs.len() == 1 => {
                self.transform_deferred_operand(&bounds.ubs[0], pol, depth + 1)
            }
            Ty::Union(types) if types.len() <= 4 => Ty::from_types(
                types
                    .iter()
                    .map(|ty| self.transform_deferred_operand(ty, pol, depth + 1))
                    .collect::<Vec<_>>()
                    .into_iter(),
            ),
            Ty::Dict(record) if record.types.len() <= 16 => {
                let mut record = record.as_ref().clone();
                record.types = record
                    .types
                    .iter()
                    .map(|ty| self.transform_deferred_operand(ty, pol, depth + 1))
                    .collect::<Vec<_>>()
                    .into();
                Ty::Dict(record.into())
            }
            Ty::Array(elem) => {
                Ty::Array(self.transform_deferred_operand(elem, pol, depth + 1).into())
            }
            Ty::Tuple(types) if types.len() <= 16 => Ty::Tuple(
                types
                    .iter()
                    .map(|ty| self.transform_deferred_operand(ty, pol, depth + 1))
                    .collect::<Vec<_>>()
                    .into(),
            ),
            Ty::Select(sel) => Ty::Select(SelectTy::new(
                self.transform_deferred_operand(&sel.ty, pol, depth + 1)
                    .into(),
                sel.select.clone(),
            )),
            Ty::Apply(apply) if depth < 6 => Ty::Apply(ApplyTy::new(
                self.transform_deferred_operand(&apply.callee, pol, depth + 1)
                    .into(),
                self.transform_apply_args(&apply.args, pol),
            )),
            Ty::Dict(_)
            | Ty::Tuple(_)
            | Ty::Func(_)
            | Ty::Args(_)
            | Ty::Pattern(_)
            | Ty::With(_)
            | Ty::Apply(_)
            | Ty::Unary(_)
            | Ty::Binary(_)
            | Ty::If(_)
            | Ty::Union(_)
            | Ty::Let(_) => Ty::Any,
        }
    }

    /// Transforms the given sequence of types.
    fn transform_seq(&mut self, types: &[Ty], pol: bool) -> Interned<Vec<Ty>> {
        let seq = types.iter().map(|ty| self.transform(ty, pol));
        seq.collect::<Vec<_>>().into()
    }

    fn fold_binary(op: BinaryOp, lhs: Ty, rhs: Ty) -> Ty {
        match op {
            BinaryOp::Add => {
                if matches!(lhs, Ty::Builtin(BuiltinTy::None)) {
                    return rhs;
                }
                if matches!(rhs, Ty::Builtin(BuiltinTy::None)) {
                    return lhs;
                }
                if let Ty::Value(lhs_val) = &lhs
                    && let Ty::Value(rhs_val) = &rhs
                    && let Value::Str(lhs_str) = &lhs_val.val
                    && let Value::Str(rhs_str) = &rhs_val.val
                {
                    let mut combined = EcoString::with_capacity(lhs_str.len() + rhs_str.len());
                    combined.push_str(lhs_str.as_str());
                    combined.push_str(rhs_str.as_str());
                    return Ty::Value(InsTy::new(Value::Str(combined.into())));
                }

                Ty::Binary(TypeBinary::new(op, lhs, rhs))
            }
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
                Ty::Binary(TypeBinary::new(op, lhs, rhs))
            }
            BinaryOp::Eq | BinaryOp::Neq => {
                if let Some(eq) = Self::known_equal(&lhs, &rhs) {
                    return Ty::Boolean(Some(if op == BinaryOp::Eq { eq } else { !eq }));
                }

                Ty::Boolean(None)
            }
            BinaryOp::Leq | BinaryOp::Geq | BinaryOp::Lt | BinaryOp::Gt => Ty::Boolean(None),
            BinaryOp::And | BinaryOp::Or => {
                match (Self::known_bool(&lhs), Self::known_bool(&rhs), op) {
                    (Some(lhs), Some(rhs), BinaryOp::And) => Ty::Boolean(Some(lhs && rhs)),
                    (Some(lhs), Some(rhs), BinaryOp::Or) => Ty::Boolean(Some(lhs || rhs)),
                    _ => Ty::Boolean(None),
                }
            }
            BinaryOp::In | BinaryOp::NotIn => Ty::Boolean(None),
            BinaryOp::Assign
            | BinaryOp::AddAssign
            | BinaryOp::SubAssign
            | BinaryOp::MulAssign
            | BinaryOp::DivAssign => Ty::Builtin(BuiltinTy::None),
        }
    }

    fn known_bool(ty: &Ty) -> Option<bool> {
        match ty {
            Ty::Boolean(value) => *value,
            Ty::Value(ins) => match &ins.val {
                Value::Bool(value) => Some(*value),
                _ => None,
            },
            _ => None,
        }
    }

    fn known_equal(lhs: &Ty, rhs: &Ty) -> Option<bool> {
        match (lhs, rhs) {
            (Ty::Value(lhs), Ty::Value(rhs)) => Some(lhs.val == rhs.val),
            (Ty::Value(lhs), Ty::Boolean(Some(rhs))) | (Ty::Boolean(Some(rhs)), Ty::Value(lhs)) => {
                match &lhs.val {
                    Value::Bool(lhs) => Some(lhs == rhs),
                    _ => Some(false),
                }
            }
            (Ty::Boolean(Some(lhs)), Ty::Boolean(Some(rhs))) => Some(lhs == rhs),
            (Ty::Builtin(BuiltinTy::None), Ty::Value(rhs))
            | (Ty::Value(rhs), Ty::Builtin(BuiltinTy::None)) => Some(rhs.val == Value::None),
            (Ty::Builtin(BuiltinTy::None), Ty::Builtin(BuiltinTy::None)) => Some(true),
            _ => None,
        }
    }

    fn contains_any(ty: &Ty) -> bool {
        match ty {
            Ty::Any => true,
            Ty::Param(param) => Self::contains_any(&param.ty),
            Ty::Array(elem) => Self::contains_any(elem),
            Ty::Tuple(types) | Ty::Union(types) => types.iter().any(Self::contains_any),
            Ty::Dict(record) => record.interface().any(|(_, ty)| Self::contains_any(ty)),
            Ty::Func(sig) | Ty::Args(sig) | Ty::Pattern(sig) => {
                sig.inputs().any(Self::contains_any)
                    || sig.body.as_ref().is_some_and(Self::contains_any)
            }
            Ty::With(with) => {
                Self::contains_any(&with.sig)
                    || with.with.inputs().any(Self::contains_any)
                    || with.with.body.as_ref().is_some_and(Self::contains_any)
            }
            Ty::Apply(apply) => {
                Self::contains_any(&apply.callee)
                    || apply.args.inputs().any(Self::contains_any)
                    || apply.args.body.as_ref().is_some_and(Self::contains_any)
            }
            Ty::Select(select) => Self::contains_any(&select.ty),
            Ty::Unary(unary) => Self::contains_any(&unary.lhs),
            Ty::Binary(binary) => {
                let [lhs, rhs] = binary.operands();
                Self::contains_any(lhs) || Self::contains_any(rhs)
            }
            Ty::If(if_ty) => {
                Self::contains_any(&if_ty.cond)
                    || Self::contains_any(&if_ty.then)
                    || Self::contains_any(&if_ty.else_)
            }
            Ty::Let(bounds) => bounds
                .lbs
                .iter()
                .chain(bounds.ubs.iter())
                .any(Self::contains_any),
            Ty::Var(_) | Ty::Boolean(_) | Ty::Builtin(_) | Ty::Value(_) => false,
        }
    }

    /// Transforms the given let type.
    #[allow(clippy::mutable_key_type)]
    fn transform_let<'a>(
        &mut self,
        lbs_iter: impl ExactSizeIterator<Item = &'a Ty>,
        ubs_iter: impl ExactSizeIterator<Item = &'a Ty>,
        decl: Option<&DeclExpr>,
        pol: bool,
    ) -> Ty {
        let mut lbs = HashSet::with_capacity(lbs_iter.len());
        let mut ubs = HashSet::with_capacity(ubs_iter.len());

        crate::log_debug_ct!("transform let [principal={}]", self.principal);

        let doc_annotated = decl.is_some_and(|decl| self.doc_annotated_vars.contains(decl));
        if !self.principal || ((pol) && !decl.is_some_and(|decl| self.negatives.contains(decl))) {
            for lb in lbs_iter {
                lbs.insert(self.transform(lb, pol));
            }
        }
        if !self.principal
            || doc_annotated
            || ((!pol) && !decl.is_some_and(|decl| self.positives.contains(decl)))
        {
            for ub in ubs_iter {
                ubs.insert(self.transform(ub, !pol));
            }
        }

        if !lbs.is_empty() {
            ubs.remove(&Ty::Any);
        }
        if !ubs.is_empty() {
            lbs.remove(&Ty::Any);
        }
        if lbs.len() == 1
            && ubs.len() == 1
            && let Some(lb) = lbs.iter().next()
            && ubs.contains(lb)
        {
            return lb.clone();
        }

        if ubs.is_empty() {
            if lbs.len() == 1 {
                return lbs.into_iter().next().unwrap();
            }
            if lbs.is_empty() {
                return Ty::Any;
            }
        } else if lbs.is_empty() && ubs.len() == 1 {
            return ubs.into_iter().next().unwrap();
        }

        // todo: bad performance
        let mut lbs: Vec<_> = lbs.into_iter().collect();
        lbs.sort();
        let mut ubs: Vec<_> = ubs.into_iter().collect();
        ubs.sort();

        Ty::Let(TypeBounds { lbs, ubs }.into())
    }

    /// Transforms the given signature.
    fn transform_sig(&mut self, sig: &SigTy, pol: bool) -> Interned<SigTy> {
        let mut sig = sig.clone();
        sig.inputs = self.transform_seq(&sig.inputs, !pol);
        if let Some(ret) = &sig.body {
            sig.body = Some(self.transform(ret, pol));
        }

        // todo: we can reduce one clone by early compare on sig.types
        sig.into()
    }

    fn transform_apply_args(&mut self, args: &SigTy, pol: bool) -> Interned<SigTy> {
        let mut args = args.clone();
        args.inputs = self.transform_seq(&args.inputs, pol);
        if let Some(ret) = &args.body {
            args.body = Some(self.transform(ret, pol));
        }
        args.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::Decl;

    /// See https://github.com/typst/typst/issues/6285
    #[test]
    fn test_simplify_sort() {
        fn ch(it: &str) -> Ty {
            Ty::Value(InsTy::new(Value::Str(it.into())))
        }

        fn val(it: Value) -> Ty {
            Ty::Value(InsTy::new(it))
        }

        fn test_sort_ty(mut tys: Vec<Ty>) {
            tys.sort();
        }

        let abcdef = vec![ch("a"), ch("b"), ch("c"), ch("d"), ch("e"), ch("f")];

        let mut res = vec![];
        res.extend(abcdef.clone());
        res.extend(abcdef.clone());
        res.extend(abcdef.clone());
        res.extend(vec![ch("c"), val(Value::None), ch("a")]);

        test_sort_ty(res);
    }

    fn var(name: &str) -> TypeVarBounds {
        TypeVarBounds::new(
            TypeVar {
                name: name.into(),
                def: Decl::lit(name).into(),
            },
            DynTypeBounds::default(),
        )
    }

    fn recursive_fun(root: &Interned<TypeVar>, depth: usize) -> Ty {
        let mut body = Ty::Var(root.clone());
        for _ in 0..depth {
            body = Ty::Func(SigTy::unary(Ty::Any, body));
        }
        body
    }

    #[test]
    fn test_recursive_cycle_union_is_not_aligned_like_simple_sub() {
        let mut info = TypeInfo::default();

        let one = var("one");
        let two = var("two");

        let one_ty = one.as_type();
        let two_ty = two.as_type();

        info.vars.insert(one.var.def.clone(), one.clone());
        info.vars.insert(two.var.def.clone(), two.clone());

        info.vars
            .get(&one.var.def)
            .unwrap()
            .bounds
            .bounds()
            .write()
            .lbs
            .insert_mut(recursive_fun(&one.var, 1));
        info.vars
            .get(&two.var.def)
            .unwrap()
            .bounds
            .bounds()
            .write()
            .lbs
            .insert_mut(recursive_fun(&two.var, 2));

        let merged = Ty::from_types([one_ty, two_ty].into_iter());
        let simplified = info.simplify(merged, true);
        assert_eq!(
            format!("{simplified:?}"),
            "((Any) => Any | (Any) => (Any) => Any)"
        );
    }

    #[test]
    fn test_simplify_populates_top_level_cache() {
        let mut info = TypeInfo::default();
        let one = var("one");
        let one_ty = one.as_type();
        info.vars.insert(one.var.def.clone(), one.clone());
        info.vars
            .get(&one.var.def)
            .unwrap()
            .bounds
            .bounds()
            .write()
            .lbs
            .insert_mut(recursive_fun(&one.var, 1));

        let _ = info.simplify(one_ty.clone(), true);
        let first_cache_len = info.cano_cache.lock().cano_cache.len();
        let _ = info.simplify(one_ty, true);
        let second_cache_len = info.cano_cache.lock().cano_cache.len();
        assert!(
            first_cache_len > 0,
            "simplify should memoize the top-level result"
        );
        assert_eq!(first_cache_len, second_cache_len);
    }
}
