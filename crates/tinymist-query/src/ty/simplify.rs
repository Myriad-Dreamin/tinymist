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

impl TypeScheme {
    /// Simplify (Canonicalize) the given type with the given type scheme.
    pub fn simplify(&self, ty: Ty, principal: bool) -> Ty {
        let mut c = self.cano_cache.lock();
        let c = &mut *c;

        c.cano_local_cache.clear();
        c.positives.clear();
        c.negatives.clear();

        let mut worker = TypeSimplifier {
            principal,
            vars: &self.vars,
            cano_cache: &mut c.cano_cache,
            cano_local_cache: &mut c.cano_local_cache,

            positives: &mut c.positives,
            negatives: &mut c.negatives,
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

impl<'a, 'b> TypeSimplifier<'a, 'b> {
    fn simplify(&mut self, ty: Ty, principal: bool) -> Ty {
        if let Some(cano) = self.cano_cache.get(&(ty.clone(), principal)) {
            return cano.clone();
        }

        self.analyze(&ty, true);

        self.transform(&ty, true)
    }

    fn analyze(&mut self, ty: &Ty, pol: bool) {
        match ty {
            Ty::Var(v) => {
                let w = self.vars.get(&v.def).unwrap();
                match &w.bounds {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let w = w.read();
                        let inserted = if pol {
                            self.positives.insert(v.def.clone())
                        } else {
                            self.negatives.insert(v.def.clone())
                        };
                        if !inserted {
                            return;
                        }

                        if pol {
                            for lb in w.lbs.iter() {
                                self.analyze(lb, pol);
                            }
                        } else {
                            for ub in w.ubs.iter() {
                                self.analyze(ub, pol);
                            }
                        }
                    }
                }
            }
            Ty::Func(f) => {
                for p in f.inputs() {
                    self.analyze(p, !pol);
                }
                if let Some(ret) = &f.body {
                    self.analyze(ret, pol);
                }
            }
            Ty::Dict(r) => {
                for p in r.types.iter() {
                    self.analyze(p, pol);
                }
            }
            Ty::Tuple(e) => {
                for ty in e.iter() {
                    self.analyze(ty, pol);
                }
            }
            Ty::Array(e) => {
                self.analyze(e, pol);
            }
            Ty::With(w) => {
                self.analyze(&w.sig, pol);
                for p in w.with.inputs() {
                    self.analyze(p, pol);
                }
            }
            Ty::Args(args) => {
                for p in args.inputs() {
                    self.analyze(p, pol);
                }
            }
            Ty::Pattern(args) => {
                for p in args.inputs() {
                    self.analyze(p, pol);
                }
            }
            Ty::Unary(u) => self.analyze(&u.lhs, pol),
            Ty::Binary(b) => {
                let [lhs, rhs] = b.operands();
                self.analyze(lhs, pol);
                self.analyze(rhs, pol);
            }
            Ty::If(i) => {
                self.analyze(&i.cond, pol);
                self.analyze(&i.then, pol);
                self.analyze(&i.else_, pol);
            }
            Ty::Union(v) => {
                for ty in v.iter() {
                    self.analyze(ty, pol);
                }
            }
            Ty::Select(a) => {
                self.analyze(&a.ty, pol);
            }
            Ty::Let(v) => {
                for lb in v.lbs.iter() {
                    self.analyze(lb, !pol);
                }
                for ub in v.ubs.iter() {
                    self.analyze(ub, pol);
                }
            }
            Ty::Param(v) => {
                self.analyze(&v.ty, pol);
            }
            Ty::Value(_v) => {}
            Ty::Any => {}
            Ty::Boolean(_) => {}
            Ty::Builtin(_) => {}
        }
    }

    fn transform(&mut self, ty: &Ty, pol: bool) -> Ty {
        match ty {
            Ty::Let(w) => self.transform_let(w, None, pol),
            Ty::Var(v) => {
                if let Some(cano) = self.cano_local_cache.get(&(v.def.clone(), self.principal)) {
                    return cano.clone();
                }
                // todo: avoid cycle
                self.cano_local_cache
                    .insert((v.def.clone(), self.principal), Ty::Any);

                let res = match &self.vars.get(&v.def).unwrap().bounds {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let w = w.read();

                        self.transform_let(&w, Some(&v.def), pol)
                    }
                };

                self.cano_local_cache
                    .insert((v.def.clone(), self.principal), res.clone());

                res
            }
            Ty::Func(f) => Ty::Func(self.transform_sig(f, pol)),
            Ty::Dict(f) => {
                let mut f = f.as_ref().clone();
                f.types = self.transform_seq(&f.types, pol);

                Ty::Dict(f.into())
            }
            Ty::Tuple(e) => Ty::Tuple(self.transform_seq(e, pol)),
            Ty::Array(e) => Ty::Array(self.transform(e, pol).into()),
            Ty::With(w) => {
                let sig = self.transform(&w.sig, pol).into();
                // Negate the pol to make correct covariance
                let with = self.transform_sig(&w.with, !pol);

                Ty::With(SigWithTy::new(sig, with))
            }
            // Negate the pol to make correct covariance
            // todo: negate?
            Ty::Args(args) => Ty::Args(self.transform_sig(args, !pol)),
            Ty::Pattern(args) => Ty::Pattern(self.transform_sig(args, !pol)),
            Ty::Unary(u) => Ty::Unary(TypeUnary::new(u.op, self.transform(&u.lhs, pol))),
            Ty::Binary(b) => {
                let [lhs, rhs] = b.operands();
                let lhs = self.transform(lhs, pol);
                let rhs = self.transform(rhs, pol);

                Ty::Binary(TypeBinary::new(b.op, lhs, rhs))
            }
            Ty::If(i) => Ty::If(IfTy::new(
                self.transform(&i.cond, pol).into(),
                self.transform(&i.then, pol).into(),
                self.transform(&i.else_, pol).into(),
            )),
            Ty::Union(seq) => {
                let seq = seq.iter().map(|ty| self.transform(ty, pol));
                let seq_no_any = seq.filter(|ty| !matches!(ty, Ty::Any));
                let seq = seq_no_any.collect::<Vec<_>>();
                Ty::from_types(seq.into_iter())
            }
            Ty::Param(ty) => {
                let mut ty = ty.as_ref().clone();
                ty.ty = self.transform(&ty.ty, pol);

                Ty::Param(ty.into())
            }
            Ty::Select(sel) => {
                let mut sel = sel.as_ref().clone();
                sel.ty = self.transform(&sel.ty, pol).into();

                Ty::Select(sel.into())
            }

            Ty::Value(v) => Ty::Value(v.clone()),
            Ty::Any => Ty::Any,
            Ty::Boolean(b) => Ty::Boolean(*b),
            Ty::Builtin(b) => Ty::Builtin(b.clone()),
        }
    }

    fn transform_seq(&mut self, seq: &[Ty], pol: bool) -> Interned<Vec<Ty>> {
        let seq = seq.iter().map(|ty| self.transform(ty, pol));
        seq.collect::<Vec<_>>().into()
    }

    #[allow(clippy::mutable_key_type)]
    fn transform_let(&mut self, w: &TypeBounds, def_id: Option<&DeclExpr>, pol: bool) -> Ty {
        let mut lbs = HashSet::with_capacity(w.lbs.len());
        let mut ubs = HashSet::with_capacity(w.ubs.len());

        log::debug!("transform let [principal={}] with {w:?}", self.principal);

        if !self.principal || ((pol) && !def_id.is_some_and(|i| self.negatives.contains(i))) {
            for lb in w.lbs.iter() {
                lbs.insert(self.transform(lb, pol));
            }
        }
        if !self.principal || ((!pol) && !def_id.is_some_and(|i| self.positives.contains(i))) {
            for ub in w.ubs.iter() {
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

        let mut lbs = lbs.into_iter().collect();
        let mut ubs = ubs.into_iter().collect();
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
