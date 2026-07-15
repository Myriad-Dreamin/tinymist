use crate::syntax::UnaryOp;
use crate::ty::def::*;

/// A trait to mutate a type.
pub trait TyMutator {
    /// Mutates the given type.
    fn mutate(&mut self, ty: &Ty, pol: bool) -> Option<Ty> {
        self.mutate_rec(ty, pol)
    }

    /// Mutates the given type recursively.
    fn mutate_rec(&mut self, ty: &Ty, pol: bool) -> Option<Ty> {
        use Ty::*;
        match ty {
            Value(..) | Any | Boolean(..) | Builtin(..) => None,
            Union(v) => Some(Union(self.mutate_vec(v, pol)?)),
            Var(..) | Let(..) => None,
            Array(arr) => Some(Array(self.mutate(arr, pol)?.into())),
            Dict(dict) => Some(Dict(self.mutate_record(dict, pol)?.into())),
            Tuple(tup) => self.mutate_tuple(tup, pol),
            Func(func) => Some(Func(self.mutate_func(func, pol)?.into())),
            Args(args) => Some(Args(self.mutate_func(args, pol)?.into())),
            Pattern(pat) => Some(Pattern(self.mutate_func(pat, pol)?.into())),
            Param(param) => Some(Param(self.mutate_param(param, pol)?.into())),
            Select(sel) => Some(Select(self.mutate_select(sel, pol)?.into())),
            With(sig) => Some(With(self.mutate_with_sig(sig, pol)?.into())),
            Unary(unary) => self.mutate_unary_ty(unary, pol),
            Binary(binary) => Some(Binary(self.mutate_binary(binary, pol)?.into())),
            If(if_expr) => Some(If(self.mutate_if(if_expr, pol)?.into())),
        }
    }

    /// Mutates the given vector of types.
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

        if mutated { Some(types.into()) } else { None }
    }

    /// Mutates and normalizes a tuple type.
    fn mutate_tuple(&mut self, ty: &[Ty], pol: bool) -> Option<Ty> {
        let mut mutated = false;
        let mut types = Vec::with_capacity(ty.len());

        for ty in ty.iter() {
            let ty = match self.mutate(ty, pol) {
                Some(ty) => {
                    mutated = true;
                    ty
                }
                None => ty.clone(),
            };

            if Self::push_spread_tuple_elements(&mut types, &ty) {
                mutated = true;
            } else {
                types.push(ty);
            }
        }

        mutated.then(|| Ty::Tuple(types.into()))
    }

    /// Pushes known tuple elements from an internal spread marker.
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

    /// Mutates the given option of type.
    fn mutate_option(&mut self, ty: Option<&Ty>, pol: bool) -> Option<Option<Ty>> {
        match ty {
            Some(ty) => self.mutate(ty, pol).map(Some),
            None => None,
        }
    }

    /// Mutates the given function signature.
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

    /// Mutates the given parameter type.
    fn mutate_param(&mut self, param: &Interned<ParamTy>, pol: bool) -> Option<ParamTy> {
        let ty = self.mutate(&param.ty, pol)?;
        let mut param = param.as_ref().clone();
        param.ty = ty;
        Some(param)
    }

    /// Mutates the given record type.
    fn mutate_record(&mut self, record: &Interned<RecordTy>, pol: bool) -> Option<RecordTy> {
        let types = self.mutate_vec(&record.types, pol)?;

        let rec = record.as_ref().clone();
        Some(RecordTy { types, ..rec })
    }

    /// Mutates the given function signature with type.
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

    /// Mutates the given unary type.
    fn mutate_unary_ty(&mut self, ty: &Interned<TypeUnary>, pol: bool) -> Option<Ty> {
        let lhs = self.mutate(&ty.lhs, pol)?;
        if ty.op == UnaryOp::ElementOf
            && let Some(elem) = Self::known_element_type(&lhs)
        {
            return Some(elem);
        }

        Some(Ty::Unary(TypeUnary { lhs, op: ty.op }.into()))
    }

    /// Gets the known element type of an iterable-like type.
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

    /// Gets known element types from multiple iterable-like types.
    fn known_element_types<'a>(types: impl Iterator<Item = &'a Ty>) -> Option<Ty> {
        let types = types
            .filter_map(Self::known_element_type)
            .collect::<Vec<_>>();
        (!types.is_empty()).then(|| Ty::from_types(types.into_iter()))
    }

    /// Gets the known element type of a tuple.
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

    /// Gets the known positional element type of arguments.
    fn known_args_element_type(args: &ArgsTy) -> Option<Ty> {
        let mut types = args.positional_params().cloned().collect::<Vec<_>>();
        if let Some(rest) = args.rest_param()
            && let Some(elem) = Self::known_element_type(rest)
        {
            types.push(elem);
        }

        (!types.is_empty()).then(|| Ty::from_types(types.into_iter()))
    }

    /// Mutates the given binary type.
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

    /// Mutates the given if type.
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

    /// Mutates the given select type.
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
    /// Mutates the given type.
    pub fn mutate(&self, pol: bool, checker: &mut impl TyMutator) -> Option<Ty> {
        checker.mutate(self, pol)
    }
}
