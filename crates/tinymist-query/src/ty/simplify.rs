#![allow(unused)]

use std::collections::HashSet;

use ecow::EcoVec;
use reflexo::hash::hash128;

use crate::{adt::interner::Interned, analysis::*, ty::def::*};

#[derive(Default)]
struct CompactTy {
    equiv_vars: HashSet<DefId>,
    primitives: HashSet<Ty>,
    recursives: HashMap<DefId, CompactTy>,
    signatures: Vec<Interned<SigTy>>,

    is_final: bool,
}

impl TypeCheckInfo {
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

    vars: &'a HashMap<DefId, TypeVarBounds>,

    cano_cache: &'b mut HashMap<(Ty, bool), Ty>,
    cano_local_cache: &'b mut HashMap<(DefId, bool), Ty>,
    negatives: &'b mut HashSet<DefId>,
    positives: &'b mut HashSet<DefId>,
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
                            self.positives.insert(v.def)
                        } else {
                            self.negatives.insert(v.def)
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
                if let Some(ret) = &f.ret {
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
            Ty::Unary(u) => self.analyze(&u.lhs, pol),
            Ty::Binary(b) => {
                let (lhs, rhs) = b.repr();
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
            Ty::Field(v) => {
                self.analyze(&v.field, pol);
            }
            Ty::Value(_v) => {}
            Ty::Clause => {}
            Ty::Undef => {}
            Ty::Content => {}
            Ty::Any => {}
            Ty::None => {}
            Ty::Infer => {}
            Ty::FlowNone => {}
            Ty::Space => {}
            Ty::Auto => {}
            Ty::Boolean(_) => {}
            Ty::Builtin(_) => {}
        }
    }

    fn transform(&mut self, ty: &Ty, pol: bool) -> Ty {
        match ty {
            Ty::Let(w) => self.transform_let(w, None, pol),
            Ty::Var(v) => {
                if let Some(cano) = self.cano_local_cache.get(&(v.def, self.principal)) {
                    return cano.clone();
                }
                // todo: avoid cycle
                self.cano_local_cache
                    .insert((v.def, self.principal), Ty::Any);

                let res = match &self.vars.get(&v.def).unwrap().bounds {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let w = w.read();

                        self.transform_let(&w, Some(&v.def), pol)
                    }
                };

                self.cano_local_cache
                    .insert((v.def, self.principal), res.clone());

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
            Ty::Args(args) => Ty::Args(self.transform_sig(args, !pol)),
            Ty::Unary(u) => Ty::Unary(TypeUnary::new(u.op, self.transform(&u.lhs, pol).into())),
            Ty::Binary(b) => {
                let (lhs, rhs) = b.repr();
                let lhs = self.transform(lhs, pol);
                let rhs = self.transform(rhs, pol);

                Ty::Binary(TypeBinary::new(b.op, lhs.into(), rhs.into()))
            }
            Ty::If(i) => Ty::If(IfTy::new(
                self.transform(&i.cond, pol).into(),
                self.transform(&i.then, pol).into(),
                self.transform(&i.else_, pol).into(),
            )),
            Ty::Union(v) => Ty::Union(self.transform_seq(v, pol)),
            Ty::Field(ty) => {
                let mut ty = ty.as_ref().clone();
                ty.field = self.transform(&ty.field, pol);

                Ty::Field(ty.into())
            }
            Ty::Select(sel) => {
                let mut sel = sel.as_ref().clone();
                sel.ty = self.transform(&sel.ty, pol).into();

                Ty::Select(sel.into())
            }

            Ty::Value(v) => Ty::Value(v.clone()),
            Ty::Clause => Ty::Clause,
            Ty::Undef => Ty::Undef,
            Ty::Content => Ty::Content,
            Ty::Any => Ty::Any,
            Ty::None => Ty::None,
            Ty::Infer => Ty::Infer,
            Ty::FlowNone => Ty::FlowNone,
            Ty::Space => Ty::Space,
            Ty::Auto => Ty::Auto,
            Ty::Boolean(b) => Ty::Boolean(*b),
            Ty::Builtin(b) => Ty::Builtin(b.clone()),
        }
    }

    fn transform_seq(&mut self, seq: &[Ty], pol: bool) -> Interned<Vec<Ty>> {
        let seq = seq.iter().map(|ty| self.transform(ty, pol));
        seq.collect::<Vec<_>>().into()
    }

    // todo: reduce duplication
    fn transform_let(&mut self, w: &TypeBounds, def_id: Option<&DefId>, pol: bool) -> Ty {
        let mut lbs = EcoVec::with_capacity(w.lbs.len());
        let mut ubs = EcoVec::with_capacity(w.ubs.len());

        log::debug!("transform let [principal={}] with {w:?}", self.principal);

        if !self.principal || ((pol) && !def_id.is_some_and(|i| self.negatives.contains(i))) {
            for lb in w.lbs.iter() {
                lbs.push(self.transform(lb, pol));
            }
        }
        if !self.principal || ((!pol) && !def_id.is_some_and(|i| self.positives.contains(i))) {
            for ub in w.ubs.iter() {
                ubs.push(self.transform(ub, !pol));
            }
        }

        if ubs.is_empty() {
            if lbs.len() == 1 {
                return lbs.pop().unwrap();
            }
            if lbs.is_empty() {
                return Ty::Any;
            }
        }

        Ty::Let(TypeBounds { lbs, ubs }.into())
    }

    fn transform_sig(&mut self, sig: &SigTy, pol: bool) -> Interned<SigTy> {
        let mut sig = sig.clone();
        sig.types = self.transform_seq(&sig.types, !pol);
        if let Some(ret) = &sig.ret {
            sig.ret = Some(self.transform(ret, pol));
        }

        // todo: we can reduce one clone by early compare on sig.types
        sig.into()
    }
}
