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
        let types = self.mutate_vec(&ty.inputs, pol);
        let ret = self.mutate_option(ty.body.as_ref(), pol);

        if types.is_none() && ret.is_none() {
            return None;
        }

        let sig = ty.as_ref().clone();
        let types = types.unwrap_or_else(|| ty.inputs.clone());
        let ret = ret.unwrap_or_else(|| ty.body.clone());
        Some(SigTy {
            inputs: types,
            body: ret,
            ..sig
        })
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
    /// Mutate the given type.
    pub fn mutate(&self, pol: bool, checker: &mut impl MutateDriver) -> Option<Ty> {
        let mut worker = Mutator;
        worker.ty(self, pol, checker)
    }
}

struct Mutator;

impl Mutator {
    fn ty(&mut self, ty: &Ty, pol: bool, mutator: &mut impl MutateDriver) -> Option<Ty> {
        use Ty::*;
        match ty {
            Value(..) | Any | Boolean(..) | Builtin(..) => mutator.mutate(ty, pol),
            Union(v) => Some(Union(mutator.mutate_vec(v, pol)?)),
            Var(..) | Let(..) => mutator.mutate(ty, pol),
            Array(e) => Some(Array(mutator.mutate(e, pol)?.into())),
            Dict(r) => Some(Dict(mutator.mutate_record(r, pol)?.into())),
            Tuple(e) => Some(Tuple(mutator.mutate_vec(e, pol)?)),
            Func(f) => Some(Func(mutator.mutate_func(f, pol)?.into())),
            Args(args) => Some(Args(mutator.mutate_func(args, pol)?.into())),
            Field(f) => {
                let field = f.field.mutate(pol, mutator)?;
                let mut f = f.as_ref().clone();
                f.field = field;
                Some(Field(f.into()))
            }
            Select(s) => Some(Select(mutator.mutate_select(s, pol)?.into())),
            With(w) => Some(With(mutator.mutate_with_sig(w, pol)?.into())),
            Unary(u) => Some(Unary(mutator.mutate_unary(u, pol)?.into())),
            Binary(b) => Some(Binary(mutator.mutate_binary(b, pol)?.into())),
            If(i) => Some(If(mutator.mutate_if(i, pol)?.into())),
        }
    }
}
