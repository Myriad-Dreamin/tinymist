#![allow(unused)]

use ecow::EcoVec;

use crate::syntax::UnaryOp;
use crate::{syntax::DeclExpr, ty::prelude::*};

/// A compact type.
#[derive(Default)]
struct CompactTy {
    equiv_vars: HashSet<DefId>,
    primitives: HashSet<Ty>,
    recursives: HashMap<DefId, CompactTy>,
    signatures: Vec<Interned<SigTy>>,

    is_final: bool,
}

#[allow(clippy::mutable_key_type)]
fn collect_input_type_vars(ty: &Ty, vars: &mut FxHashSet<DeclExpr>, traversed: &mut FxHashSet<Ty>) {
    if !traversed.insert(ty.clone()) {
        return;
    }

    match ty {
        Ty::Var(var) => {
            vars.insert(var.def.clone());
        }
        Ty::Func(_) | Ty::With(_) => {}
        Ty::Param(param) => collect_input_type_vars(&param.ty, vars, traversed),
        Ty::Union(types) | Ty::Tuple(types) => {
            for ty in types.iter() {
                collect_input_type_vars(ty, vars, traversed);
            }
        }
        Ty::Let(bounds) => {
            for ty in bounds.lbs.iter().chain(&bounds.ubs) {
                collect_input_type_vars(ty, vars, traversed);
            }
        }
        Ty::Dict(record) => {
            for ty in record.types.iter() {
                collect_input_type_vars(ty, vars, traversed);
            }
        }
        Ty::Array(elem) => collect_input_type_vars(elem, vars, traversed),
        Ty::Args(sig) | Ty::Pattern(sig) => {
            for input in sig.inputs() {
                collect_input_type_vars(input, vars, traversed);
            }
        }
        Ty::Select(select) => collect_input_type_vars(&select.ty, vars, traversed),
        Ty::Unary(unary) => collect_input_type_vars(&unary.lhs, vars, traversed),
        Ty::Binary(binary) => {
            let [lhs, rhs] = binary.operands();
            collect_input_type_vars(lhs, vars, traversed);
            collect_input_type_vars(rhs, vars, traversed);
        }
        Ty::If(if_ty) => {
            collect_input_type_vars(&if_ty.cond, vars, traversed);
            collect_input_type_vars(&if_ty.then, vars, traversed);
            collect_input_type_vars(&if_ty.else_, vars, traversed);
        }
        Ty::Any | Ty::Boolean(_) | Ty::Builtin(_) | Ty::Value(_) => {}
    }
}

impl TypeInfo {
    /// Simplifies (canonicalizes) the given type with the given type scheme.
    pub fn simplify(&self, ty: Ty, principal: bool) -> Ty {
        let mut cache = self.cano_cache.lock();
        let cache = &mut *cache;
        let mut signature_binders = FxHashSet::default();

        cache.transform_cache.clear();
        cache.cano_local_cache.clear();
        cache.positives.clear();
        cache.negatives.clear();

        let mut worker = TypeSimplifier {
            principal,
            vars: &self.vars,
            cano_cache: &mut cache.cano_cache,
            transform_cache: &mut cache.transform_cache,
            cano_local_cache: &mut cache.cano_local_cache,
            analyze_cache: FxHashSet::default(),
            input_var_cache: FxHashSet::default(),

            positives: &mut cache.positives,
            negatives: &mut cache.negatives,
        };

        worker.simplify(ty, principal, &mut signature_binders)
    }
}

/// A simplifier to simplify a type.
struct TypeSimplifier<'a, 'b> {
    principal: bool,

    vars: &'a FxHashMap<DeclExpr, TypeVarBounds>,

    cano_cache: &'b mut FxHashMap<(Ty, bool), Ty>,
    transform_cache: &'b mut FxHashMap<(Ty, bool), Ty>,
    cano_local_cache: &'b mut FxHashMap<(DeclExpr, bool), Ty>,
    analyze_cache: FxHashSet<(Ty, bool)>,
    input_var_cache: FxHashSet<Ty>,
    negatives: &'b mut FxHashSet<DeclExpr>,
    positives: &'b mut FxHashSet<DeclExpr>,
}

impl TypeSimplifier<'_, '_> {
    /// Simplifies the given type.
    fn simplify(
        &mut self,
        ty: Ty,
        principal: bool,
        signature_binders: &mut FxHashSet<DeclExpr>,
    ) -> Ty {
        if let Some(cano) = self.cano_cache.get(&(ty.clone(), principal)) {
            return cano.clone();
        }

        self.analyze(&ty, true, signature_binders);
        let cano = self.transform(&ty, true, signature_binders);
        self.cano_cache.insert((ty, principal), cano.clone());
        cano
    }

    /// Analyzes the given type.
    fn analyze(&mut self, ty: &Ty, pol: bool, signature_binders: &mut FxHashSet<DeclExpr>) {
        if !self.analyze_cache.insert((ty.clone(), pol)) {
            return;
        }

        match ty {
            Ty::Var(var) => {
                if self.principal && signature_binders.contains(&var.def) {
                    return;
                }
                let Some(w) = self.vars.get(&var.def) else {
                    return;
                };

                let inserted = if pol {
                    self.positives.insert(var.def.clone())
                } else {
                    self.negatives.insert(var.def.clone())
                };
                if !inserted {
                    return;
                }

                match &w.bounds {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let bounds = w.read();
                        if pol {
                            for lb in bounds.lbs.iter() {
                                self.analyze(lb, pol, signature_binders);
                            }
                        } else {
                            for ub in bounds.ubs.iter() {
                                self.analyze(ub, pol, signature_binders);
                            }
                        }
                    }
                }
            }
            Ty::Func(func) => {
                if self.principal {
                    for input in func.inputs() {
                        collect_input_type_vars(
                            input,
                            signature_binders,
                            &mut self.input_var_cache,
                        );
                    }
                }
                for input_ty in func.inputs() {
                    self.analyze(input_ty, !pol, signature_binders);
                }
                if let Some(ret_ty) = &func.body {
                    self.analyze(ret_ty, pol, signature_binders);
                }
            }
            Ty::Dict(record) => {
                for member in record.types.iter() {
                    self.analyze(member, pol, signature_binders);
                }
            }
            Ty::Tuple(elems) => {
                for elem in elems.iter() {
                    self.analyze(elem, pol, signature_binders);
                }
            }
            Ty::Array(arr) => {
                self.analyze(arr, pol, signature_binders);
            }
            Ty::With(with) => {
                self.analyze(&with.sig, pol, signature_binders);
                for input in with.with.inputs() {
                    self.analyze(input, pol, signature_binders);
                }
            }
            Ty::Args(args) => {
                for input in args.inputs() {
                    self.analyze(input, pol, signature_binders);
                }
            }
            Ty::Pattern(pat) => {
                if self.principal {
                    for input in pat.inputs() {
                        collect_input_type_vars(
                            input,
                            signature_binders,
                            &mut self.input_var_cache,
                        );
                    }
                }
                for input in pat.inputs() {
                    self.analyze(input, pol, signature_binders);
                }
            }
            Ty::Unary(unary) => self.analyze(&unary.lhs, pol, signature_binders),
            Ty::Binary(binary) => {
                let [lhs, rhs] = binary.operands();
                self.analyze(lhs, pol, signature_binders);
                self.analyze(rhs, pol, signature_binders);
            }
            Ty::If(if_expr) => {
                self.analyze(&if_expr.cond, pol, signature_binders);
                self.analyze(&if_expr.then, pol, signature_binders);
                self.analyze(&if_expr.else_, pol, signature_binders);
            }
            Ty::Union(types) => {
                for ty in types.iter() {
                    self.analyze(ty, pol, signature_binders);
                }
            }
            Ty::Select(select) => {
                self.analyze(&select.ty, pol, signature_binders);
            }
            Ty::Let(bounds) => {
                for lb in bounds.lbs.iter() {
                    self.analyze(lb, !pol, signature_binders);
                }
                for ub in bounds.ubs.iter() {
                    self.analyze(ub, pol, signature_binders);
                }
            }
            Ty::Param(param) => {
                self.analyze(&param.ty, pol, signature_binders);
            }
            Ty::Value(_v) => {}
            Ty::Any => {}
            Ty::Boolean(_) => {}
            Ty::Builtin(_) => {}
        }
    }

    /// Transforms the given type.
    fn transform(&mut self, ty: &Ty, pol: bool, signature_binders: &FxHashSet<DeclExpr>) -> Ty {
        let cache_key = (ty.clone(), pol);
        if let Some(cano) = self.transform_cache.get(&cache_key) {
            return cano.clone();
        }

        let cano = match ty {
            Ty::Let(bounds) => self.transform_let(
                bounds.lbs.iter(),
                bounds.ubs.iter(),
                None,
                pol,
                signature_binders,
            ),
            Ty::Var(var) => {
                if self.principal && signature_binders.contains(&var.def) {
                    return Ty::Var(var.clone());
                }
                let Some(bounds) = self.vars.get(&var.def) else {
                    return Ty::Var(var.clone());
                };
                if let Some(cano) = self
                    .cano_local_cache
                    .get(&(var.def.clone(), self.principal))
                {
                    return cano.clone();
                }
                // todo: avoid cycle
                self.cano_local_cache
                    .insert((var.def.clone(), self.principal), Ty::Any);

                let res = match &bounds.bounds {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let w = w.read();

                        self.transform_let(
                            w.lbs.iter(),
                            w.ubs.iter(),
                            Some(&var.def),
                            pol,
                            signature_binders,
                        )
                    }
                };

                self.cano_local_cache
                    .insert((var.def.clone(), self.principal), res.clone());

                res
            }
            Ty::Func(func) => Ty::Func(self.transform_sig(func, pol, signature_binders)),
            Ty::Dict(record) => {
                let mut mutated = record.as_ref().clone();
                mutated.types = self.transform_seq(&mutated.types, pol, signature_binders);

                Ty::Dict(mutated.into())
            }
            Ty::Tuple(tup) => self.transform_tuple(tup, pol, signature_binders),
            Ty::Array(arr) => Ty::Array(self.transform(arr, pol, signature_binders).into()),
            Ty::With(with) => {
                let sig = self.transform(&with.sig, pol, signature_binders).into();
                // Negate the pol to make correct covariance
                let mutated = self.transform_sig(&with.with, !pol, signature_binders);

                Ty::With(SigWithTy::new(sig, mutated))
            }
            // Negate the pol to make correct covariance
            // todo: negate?
            Ty::Args(args) => Ty::Args(self.transform_sig(args, !pol, signature_binders)),
            Ty::Pattern(pat) => Ty::Pattern(self.transform_sig(pat, !pol, signature_binders)),
            Ty::Unary(unary) => self.transform_unary(unary, pol, signature_binders),
            Ty::Binary(binary) => {
                let [lhs, rhs] = binary.operands();
                let lhs = self.transform(lhs, pol, signature_binders);
                let rhs = self.transform(rhs, pol, signature_binders);

                Ty::Binary(TypeBinary::new(binary.op, lhs, rhs))
            }
            Ty::If(if_ty) => Ty::If(IfTy::new(
                self.transform(&if_ty.cond, pol, signature_binders).into(),
                self.transform(&if_ty.then, pol, signature_binders).into(),
                self.transform(&if_ty.else_, pol, signature_binders).into(),
            )),
            Ty::Union(types) => {
                let seq = types
                    .iter()
                    .map(|ty| self.transform(ty, pol, signature_binders));
                let seq_no_any = seq.filter(|ty| !matches!(ty, Ty::Any));
                let seq = seq_no_any.collect::<Vec<_>>();
                Ty::from_types(seq.into_iter())
            }
            Ty::Param(param) => {
                let mut param = param.as_ref().clone();
                param.ty = self.transform(&param.ty, pol, signature_binders);

                Ty::Param(param.into())
            }
            Ty::Select(sel) => {
                let mut sel = sel.as_ref().clone();
                sel.ty = self.transform(&sel.ty, pol, signature_binders).into();

                Ty::Select(sel.into())
            }

            Ty::Value(ins_ty) => Ty::Value(ins_ty.clone()),
            Ty::Any => Ty::Any,
            Ty::Boolean(truthiness) => Ty::Boolean(*truthiness),
            Ty::Builtin(ty) => Ty::Builtin(ty.clone()),
        };

        self.transform_cache.insert(cache_key, cano.clone());
        cano
    }

    /// Transforms the given sequence of types.
    fn transform_seq(
        &mut self,
        types: &[Ty],
        pol: bool,
        signature_binders: &FxHashSet<DeclExpr>,
    ) -> Interned<Vec<Ty>> {
        let seq = types
            .iter()
            .map(|ty| self.transform(ty, pol, signature_binders));
        seq.collect::<Vec<_>>().into()
    }

    /// Transforms the given let type.
    #[allow(clippy::mutable_key_type)]
    fn transform_let<'a>(
        &mut self,
        lbs_iter: impl ExactSizeIterator<Item = &'a Ty>,
        ubs_iter: impl ExactSizeIterator<Item = &'a Ty>,
        decl: Option<&DeclExpr>,
        pol: bool,
        signature_binders: &FxHashSet<DeclExpr>,
    ) -> Ty {
        let mut lbs = HashSet::with_capacity(lbs_iter.len());
        let mut ubs = HashSet::with_capacity(ubs_iter.len());

        crate::log_debug_ct!("transform let [principal={}]", self.principal);

        if !self.principal || ((pol) && !decl.is_some_and(|decl| self.negatives.contains(decl))) {
            for lb in lbs_iter {
                lbs.insert(self.transform(lb, pol, signature_binders));
            }
        }
        if !self.principal || ((!pol) && !decl.is_some_and(|decl| self.positives.contains(decl))) {
            for ub in ubs_iter {
                ubs.insert(self.transform(ub, !pol, signature_binders));
            }
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

    fn transform_tuple(
        &mut self,
        tup: &[Ty],
        pol: bool,
        signature_binders: &FxHashSet<DeclExpr>,
    ) -> Ty {
        let mut types = Vec::with_capacity(tup.len());

        for elem in tup.iter() {
            let elem = self.transform(elem, pol, signature_binders);
            if !Self::push_spread_tuple_elements(&mut types, &elem) {
                types.push(elem);
            }
        }

        Ty::Tuple(types.into())
    }

    fn push_spread_tuple_elements(types: &mut Vec<Ty>, ty: &Ty) -> bool {
        let Ty::Unary(unary) = ty else {
            return false;
        };
        if unary.op != UnaryOp::Spread {
            return false;
        }

        match &unary.lhs {
            Ty::Tuple(elems) => {
                types.extend(elems.iter().cloned());
                true
            }
            Ty::Args(args) => {
                types.extend(args.positional_params().cloned());
                if let Some(rest) = args.rest_param()
                    && !Self::push_spread_tuple_elements(
                        types,
                        &Ty::Unary(TypeUnary::new(UnaryOp::Spread, rest.clone())),
                    )
                {
                    types.push(Ty::Unary(TypeUnary::new(UnaryOp::Spread, rest.clone())));
                }
                true
            }
            _ => false,
        }
    }

    fn transform_unary(
        &mut self,
        unary: &TypeUnary,
        pol: bool,
        signature_binders: &FxHashSet<DeclExpr>,
    ) -> Ty {
        let lhs = self.transform(&unary.lhs, pol, signature_binders);
        if unary.op == UnaryOp::ElementOf
            && let Some(elem) = Self::known_element_type(&lhs)
        {
            return elem;
        }

        Ty::Unary(TypeUnary::new(unary.op, lhs))
    }

    fn known_element_type(ty: &Ty) -> Option<Ty> {
        match ty {
            Ty::Array(elem) => Some(elem.as_ref().clone()),
            Ty::Tuple(elems) => Self::known_tuple_element_type(elems),
            Ty::Args(args) => Self::known_args_element_type(args),
            Ty::Let(bounds) => Self::known_element_types(bounds.lbs.iter()),
            Ty::Union(types) => Self::known_element_types(types.iter()),
            _ => None,
        }
    }

    fn known_element_types<'a>(types: impl Iterator<Item = &'a Ty>) -> Option<Ty> {
        let types = types
            .filter_map(Self::known_element_type)
            .collect::<Vec<_>>();
        (!types.is_empty()).then(|| Ty::from_types(types.into_iter()))
    }

    fn known_tuple_element_type(elems: &[Ty]) -> Option<Ty> {
        let mut types = vec![];
        for elem in elems {
            if let Ty::Unary(unary) = elem
                && unary.op == UnaryOp::Spread
            {
                if let Some(elem) = Self::known_element_type(&unary.lhs) {
                    types.push(elem);
                }
                continue;
            }

            types.push(elem.clone());
        }

        (!types.is_empty()).then(|| Ty::from_types(types.into_iter()))
    }

    fn known_args_element_type(args: &ArgsTy) -> Option<Ty> {
        let mut types = args.positional_params().cloned().collect::<Vec<_>>();
        if let Some(rest) = args.rest_param()
            && let Some(elem) = Self::known_element_type(rest)
        {
            types.push(elem);
        }

        (!types.is_empty()).then(|| Ty::from_types(types.into_iter()))
    }

    /// Transforms the given signature.
    fn transform_sig(
        &mut self,
        sig: &SigTy,
        pol: bool,
        signature_binders: &FxHashSet<DeclExpr>,
    ) -> Interned<SigTy> {
        let mut sig = sig.clone();
        sig.inputs = self.transform_seq(&sig.inputs, !pol, signature_binders);
        if let Some(ret) = &sig.body {
            sig.body = Some(self.transform(ret, pol, signature_binders));
        }

        // todo: we can reduce one clone by early compare on sig.types
        sig.into()
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

    #[test]
    #[allow(clippy::mutable_key_type)]
    fn test_analyze_memoizes_shared_type_dag() {
        const DEPTH: usize = 16;

        let mut shared = Ty::Any;
        for _ in 0..DEPTH {
            shared = Ty::Tuple(vec![shared.clone(), shared].into());
        }

        let info = TypeInfo::default();
        let mut cano_cache = FxHashMap::default();
        let mut transform_cache = FxHashMap::default();
        let mut cano_local_cache = FxHashMap::default();
        let mut positives = FxHashSet::default();
        let mut negatives = FxHashSet::default();
        let mut worker = TypeSimplifier {
            principal: true,
            vars: &info.vars,
            cano_cache: &mut cano_cache,
            transform_cache: &mut transform_cache,
            cano_local_cache: &mut cano_local_cache,
            analyze_cache: FxHashSet::default(),
            input_var_cache: FxHashSet::default(),
            positives: &mut positives,
            negatives: &mut negatives,
        };
        let mut signature_binders = FxHashSet::default();

        worker.analyze(&shared, true, &mut signature_binders);

        assert_eq!(worker.analyze_cache.len(), DEPTH + 1);
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

    #[test]
    fn test_signature_inputs_are_principal_binders() {
        let binder = TypeVar::new("body".into(), Decl::lit("body").into());
        let binder_ty = Ty::Var(binder.clone());
        let content = Ty::Builtin(BuiltinTy::Content(None));
        let sig = SigTy::unary(binder_ty.clone(), binder_ty);

        let sig_ty = Ty::Func(sig);
        let mut dynamic_bounds = DynTypeBounds::default();
        dynamic_bounds.ubs.insert_mut(content);
        let mut info = TypeInfo::default();
        info.vars.insert(
            binder.def.clone(),
            TypeVarBounds::new(binder.as_ref().clone(), dynamic_bounds),
        );
        assert_eq!(
            format!("{:?}", info.simplify(sig_ty.clone(), false)),
            "(Content) => Content"
        );
        assert_eq!(
            format!("{:?}", info.simplify(sig_ty, true)),
            "(@body) => @body"
        );
        assert_eq!(info.vars.len(), 1);
    }

    #[test]
    fn test_principal_simplify_preserves_unused_signature_binder() {
        let binder = TypeVar::new("body".into(), Decl::lit("body").into());
        let binder_ty = Ty::Var(binder.clone());
        let sig = SigTy::unary(binder_ty, Ty::Builtin(BuiltinTy::Color));

        let mut dynamic_bounds = DynTypeBounds::default();
        dynamic_bounds
            .ubs
            .insert_mut(Ty::Builtin(BuiltinTy::Content(None)));
        let mut info = TypeInfo::default();
        info.vars.insert(
            binder.def.clone(),
            TypeVarBounds::new(binder.as_ref().clone(), dynamic_bounds),
        );

        assert_eq!(
            format!("{:?}", info.simplify(Ty::Func(sig), true)),
            "(@body) => Color"
        );
    }
}
