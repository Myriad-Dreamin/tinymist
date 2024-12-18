use crate::ty::def::*;

pub trait TyMutator {
    fn mutate(&mut self, ty: &Ty, pol: bool) -> Option<Ty> {
        self.mutate_rec(ty, pol)
    }
    fn mutate_rec(&mut self, ty: &Ty, pol: bool) -> Option<Ty> {
        use Ty::*;
        match ty {
            Value(..) | Any | Boolean(..) | Builtin(..) => None,
            Union(v) => Some(Union(self.mutate_vec(v, pol)?)),
            Var(..) | Let(..) => None,
            Array(arr) => Some(Array(self.mutate(arr, pol)?.into())),
            Dict(dict) => Some(Dict(self.mutate_record(dict, pol)?.into())),
            Tuple(tup) => Some(Tuple(self.mutate_vec(tup, pol)?)),
            Func(func) => Some(Func(self.mutate_func(func, pol)?.into())),
            Args(args) => Some(Args(self.mutate_func(args, pol)?.into())),
            Pattern(pat) => Some(Pattern(self.mutate_func(pat, pol)?.into())),
            Param(param) => Some(Param(self.mutate_param(param, pol)?.into())),
            Select(sel) => Some(Select(self.mutate_select(sel, pol)?.into())),
            With(sig) => Some(With(self.mutate_with_sig(sig, pol)?.into())),
            Unary(unary) => Some(Unary(self.mutate_unary(unary, pol)?.into())),
            Binary(binary) => Some(Binary(self.mutate_binary(binary, pol)?.into())),
            If(if_expr) => Some(If(self.mutate_if(if_expr, pol)?.into())),
        }
    }

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

    fn mutate_param(&mut self, f: &Interned<ParamTy>, pol: bool) -> Option<ParamTy> {
        let ty = self.mutate(&f.ty, pol)?;
        let mut f = f.as_ref().clone();
        f.ty = ty;
        Some(f)
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
        let lhs = self.mutate(&ty.lhs, pol)?;

        Some(TypeUnary { lhs, op: ty.op })
    }

    fn mutate_binary(&mut self, ty: &Interned<TypeBinary>, pol: bool) -> Option<TypeBinary> {
        let (lhs, rhs) = &ty.operands;

        let x = self.mutate(lhs, pol);
        let y = self.mutate(rhs, pol);

        if x.is_none() && y.is_none() {
            return None;
        }

        let lhs = x.unwrap_or_else(|| lhs.clone());
        let rhs = y.unwrap_or_else(|| rhs.clone());

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

impl<T> TyMutator for T
where
    T: FnMut(&Ty, bool) -> Option<Ty>,
{
    fn mutate(&mut self, ty: &Ty, pol: bool) -> Option<Ty> {
        self(ty, pol)
    }
}

impl Ty {
    /// Mutate the given type.
    pub fn mutate(&self, pol: bool, checker: &mut impl TyMutator) -> Option<Ty> {
        checker.mutate(self, pol)
    }
}
