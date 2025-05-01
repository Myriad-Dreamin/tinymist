#![allow(unused)]

use ecow::EcoVec;

use crate::{syntax::DeclExpr, ty::prelude::*};

#[derive(Default)]
struct CompactTy {
    equiv_vars: HashSet<DefId>,
    primitives: HashSet<Ty>,
    recursives: HashMap<DefId, CompactTy>,
    signatures: Vec<Interned<SigTy>>,

    is_final: bool,
}

impl TypeInfo {
    /// Simplify (Canonicalize) the given type with the given type scheme.
    pub fn simplify(&self, ty: Ty, principal: bool) -> Ty {
        let mut cache = self.cano_cache.lock();
        let cache = &mut *cache;

        cache.cano_local_cache.clear();
        cache.positives.clear();
        cache.negatives.clear();

        let mut worker = TypeSimplifier {
            principal,
            vars: &self.vars,
            cano_cache: &mut cache.cano_cache,
            cano_local_cache: &mut cache.cano_local_cache,

            positives: &mut cache.positives,
            negatives: &mut cache.negatives,
        };

        worker.simplify(ty, principal)
    }
}

struct TypeSimplifier<'a, 'b> {
    principal: bool,

    vars: &'a FxHashMap<DeclExpr, TypeVarBounds>,

    cano_cache: &'b mut FxHashMap<(Ty, bool), Ty>,
    cano_local_cache: &'b mut FxHashMap<(DeclExpr, bool), Ty>,
    negatives: &'b mut FxHashSet<DeclExpr>,
    positives: &'b mut FxHashSet<DeclExpr>,
}

impl TypeSimplifier<'_, '_> {
    fn simplify(&mut self, ty: Ty, principal: bool) -> Ty {
        if let Some(cano) = self.cano_cache.get(&(ty.clone(), principal)) {
            return cano.clone();
        }

        self.analyze(&ty, true);

        self.transform(&ty, true)
    }

    fn analyze(&mut self, ty: &Ty, pol: bool) {
        match ty {
            Ty::Var(var) => {
                let w = self.vars.get(&var.def).unwrap();
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
            Ty::Select(select) => {
                self.analyze(&select.ty, pol);
            }
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

    fn transform(&mut self, ty: &Ty, pol: bool) -> Ty {
        match ty {
            Ty::Let(bounds) => self.transform_let(bounds.lbs.iter(), bounds.ubs.iter(), None, pol),
            Ty::Var(var) => {
                if let Some(cano) = self
                    .cano_local_cache
                    .get(&(var.def.clone(), self.principal))
                {
                    return cano.clone();
                }
                // todo: avoid cycle
                self.cano_local_cache
                    .insert((var.def.clone(), self.principal), Ty::Any);

                let res = match &self.vars.get(&var.def).unwrap().bounds {
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
            // Negate the pol to make correct covariance
            // todo: negate?
            Ty::Args(args) => Ty::Args(self.transform_sig(args, !pol)),
            Ty::Pattern(pat) => Ty::Pattern(self.transform_sig(pat, !pol)),
            Ty::Unary(unary) => {
                Ty::Unary(TypeUnary::new(unary.op, self.transform(&unary.lhs, pol)))
            }
            Ty::Binary(binary) => {
                let [lhs, rhs] = binary.operands();
                let lhs = self.transform(lhs, pol);
                let rhs = self.transform(rhs, pol);

                Ty::Binary(TypeBinary::new(binary.op, lhs, rhs))
            }
            Ty::If(if_ty) => Ty::If(IfTy::new(
                self.transform(&if_ty.cond, pol).into(),
                self.transform(&if_ty.then, pol).into(),
                self.transform(&if_ty.else_, pol).into(),
            )),
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
                sel.ty = self.transform(&sel.ty, pol).into();

                Ty::Select(sel.into())
            }

            Ty::Value(ins_ty) => Ty::Value(ins_ty.clone()),
            Ty::Any => Ty::Any,
            Ty::Boolean(truthiness) => Ty::Boolean(*truthiness),
            Ty::Builtin(ty) => Ty::Builtin(ty.clone()),
        }
    }

    fn transform_seq(&mut self, types: &[Ty], pol: bool) -> Interned<Vec<Ty>> {
        let seq = types.iter().map(|ty| self.transform(ty, pol));
        seq.collect::<Vec<_>>().into()
    }

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

        if !self.principal || ((pol) && !decl.is_some_and(|decl| self.negatives.contains(decl))) {
            for lb in lbs_iter {
                lbs.insert(self.transform(lb, pol));
            }
        }
        if !self.principal || ((!pol) && !decl.is_some_and(|decl| self.positives.contains(decl))) {
            for ub in ubs_iter {
                ubs.insert(self.transform(ub, !pol));
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

    fn transform_sig(&mut self, sig: &SigTy, pol: bool) -> Interned<SigTy> {
        let mut sig = sig.clone();
        sig.inputs = self.transform_seq(&sig.inputs, !pol);
        if let Some(ret) = &sig.body {
            sig.body = Some(self.transform(ret, pol));
        }

        // todo: we can reduce one clone by early compare on sig.types
        sig.into()
    }
}
