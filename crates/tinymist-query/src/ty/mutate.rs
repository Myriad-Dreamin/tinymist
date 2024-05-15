use crate::{adt::interner::Interned, ty::def::*};

pub trait MutateDriver {
    fn mutate(&mut self, ty: &Ty, pol: bool) -> Option<Ty>;

    fn mutate_vec(&mut self, ty: &[Ty], pol: bool) -> Option<Interned<Vec<Ty>>> {
        let mut mutated = false;

        let mut types = Vec::with_capacity(ty.len());
        for ty in ty.iter() {
            match self.mutate(ty, pol) {
                Some(ty) => {
                    types.push(ty);
                    mutated = true;
                }
                None => types.push(ty.clone()),
            }
        }

        if mutated {
            Some(types.into())
        } else {
            None
        }
    }

    fn mutate_option(&mut self, ty: Option<&Ty>, pol: bool) -> Option<Option<Ty>> {
        match ty {
            Some(ty) => self.mutate(ty, pol).map(Some),
            None => None,
        }
    }

    fn mutate_func(&mut self, ty: &Interned<SigTy>, pol: bool) -> Option<SigTy> {
        let types = self.mutate_vec(&ty.types, pol);
        let ret = self.mutate_option(ty.ret.as_ref(), pol);

        if types.is_none() && ret.is_none() {
            return None;
        }

        let sig = ty.as_ref().clone();
        let types = types.unwrap_or_else(|| ty.types.clone());
        let ret = ret.unwrap_or_else(|| ty.ret.clone());
        Some(SigTy { types, ret, ..sig })
    }

    fn mutate_record(&mut self, ty: &Interned<RecordTy>, pol: bool) -> Option<RecordTy> {
        let types = self.mutate_vec(&ty.types, pol)?;

        let rec = ty.as_ref().clone();
        Some(RecordTy { types, ..rec })
    }

    fn mutate_with_sig(&mut self, ty: &Interned<SigWithTy>, pol: bool) -> Option<SigWithTy> {
        let sig = self.mutate(ty.sig.as_ref(), pol);
        let with = self.mutate_func(&ty.with, pol);

        if sig.is_none() && with.is_none() {
            return None;
        }

        let sig = sig.map(Interned::new).unwrap_or_else(|| ty.sig.clone());
        let with = with.map(Interned::new).unwrap_or_else(|| ty.with.clone());

        Some(SigWithTy { sig, with })
    }

    fn mutate_unary(&mut self, ty: &Interned<TypeUnary>, pol: bool) -> Option<TypeUnary> {
        let lhs = self.mutate(ty.lhs.as_ref(), pol)?.into();

        Some(TypeUnary { lhs, op: ty.op })
    }

    fn mutate_binary(&mut self, ty: &Interned<TypeBinary>, pol: bool) -> Option<TypeBinary> {
        let (lhs, rhs) = &ty.operands;

        let x = self.mutate(lhs, pol);
        let y = self.mutate(rhs, pol);

        if x.is_none() && y.is_none() {
            return None;
        }

        let lhs = x.map(Interned::new).unwrap_or_else(|| lhs.clone());
        let rhs = y.map(Interned::new).unwrap_or_else(|| rhs.clone());

        Some(TypeBinary {
            operands: (lhs, rhs),
            op: ty.op,
        })
    }

    fn mutate_if(&mut self, ty: &Interned<IfTy>, pol: bool) -> Option<IfTy> {
        let cond = self.mutate(ty.cond.as_ref(), pol);
        let then = self.mutate(ty.then.as_ref(), pol);
        let else_ = self.mutate(ty.else_.as_ref(), pol);

        if cond.is_none() && then.is_none() && else_.is_none() {
            return None;
        }

        let cond = cond.map(Interned::new).unwrap_or_else(|| ty.cond.clone());
        let then = then.map(Interned::new).unwrap_or_else(|| ty.then.clone());
        let else_ = else_.map(Interned::new).unwrap_or_else(|| ty.else_.clone());

        Some(IfTy { cond, then, else_ })
    }

    fn mutate_select(&mut self, ty: &Interned<SelectTy>, pol: bool) -> Option<SelectTy> {
        let target = self.mutate(ty.ty.as_ref(), pol)?.into();

        Some(SelectTy {
            ty: target,
            select: ty.select.clone(),
        })
    }
}

impl<T> MutateDriver for T
where
    T: FnMut(&Ty, bool) -> Option<Ty>,
{
    fn mutate(&mut self, ty: &Ty, pol: bool) -> Option<Ty> {
        self(ty, pol)
    }
}

impl Ty {
    pub fn mutate(&self, pol: bool, checker: &mut impl MutateDriver) -> Option<Ty> {
        let mut worker = Mutator;
        worker.ty(self, pol, checker)
    }
}

struct Mutator;

impl Mutator {
    fn ty(&mut self, ty: &Ty, pol: bool, mutator: &mut impl MutateDriver) -> Option<Ty> {
        match ty {
            Ty::Func(f) => {
                let f = mutator.mutate_func(f, pol)?;
                Some(Ty::Func(f.into()))
            }
            Ty::Dict(r) => {
                let r = mutator.mutate_record(r, pol)?;
                Some(Ty::Dict(r.into()))
            }
            Ty::Tuple(e) => {
                let e = mutator.mutate_vec(e, pol)?;
                Some(Ty::Tuple(e))
            }
            Ty::Array(e) => {
                let ty = mutator.mutate(e, pol)?;
                Some(Ty::Array(ty.into()))
            }
            Ty::With(w) => {
                let w = mutator.mutate_with_sig(w, pol)?;
                Some(Ty::With(w.into()))
            }
            Ty::Args(args) => {
                let args = mutator.mutate_func(args, pol)?;
                Some(Ty::Args(args.into()))
            }
            Ty::Unary(u) => {
                let u = mutator.mutate_unary(u, pol)?;
                Some(Ty::Unary(u.into()))
            }
            Ty::Binary(b) => {
                let b = mutator.mutate_binary(b, pol)?;
                Some(Ty::Binary(b.into()))
            }
            Ty::If(i) => {
                let i = mutator.mutate_if(i, pol)?;
                Some(Ty::If(i.into()))
            }
            Ty::Union(v) => {
                let v = mutator.mutate_vec(v, pol)?;
                Some(Ty::Union(v))
            }
            Ty::Field(f) => {
                let field = f.field.mutate(pol, mutator)?;
                let mut f = f.as_ref().clone();
                f.field = field;
                Some(Ty::Field(f.into()))
            }
            Ty::Select(s) => {
                let s = mutator.mutate_select(s, pol)?;
                Some(Ty::Select(s.into()))
            }
            Ty::Var(..)
            | Ty::Let(..)
            | Ty::Value(..)
            | Ty::Any
            | Ty::Boolean(..)
            | Ty::Builtin(..) => mutator.mutate(ty, pol),
        }
    }
}
