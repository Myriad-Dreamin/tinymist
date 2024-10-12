use once_cell::sync::Lazy;

use crate::{adt::interner::Interned, ty::def::*};

use super::{Sig, SigChecker, SigSurfaceKind};

pub trait ApplyChecker {
    fn apply(&mut self, sig: Sig, arguments: &Interned<ArgsTy>, pol: bool);

    fn bound_of_var(&mut self, _var: &Interned<TypeVar>, _pol: bool) -> Option<TypeBounds> {
        None
    }
}

static EMPTY_ARGS: Lazy<Interned<ArgsTy>> = Lazy::new(|| ArgsTy::default().into());

impl Ty {
    /// Call the given type with the given arguments.
    pub fn call(&self, args: &Interned<ArgsTy>, pol: bool, checker: &mut impl ApplyChecker) {
        self.apply(SigSurfaceKind::Call, args, pol, checker)
    }

    /// Get the tuple element type of the given type.
    pub fn tuple_element_of(&self, pol: bool, checker: &mut impl ApplyChecker) {
        self.apply(SigSurfaceKind::Array, &EMPTY_ARGS, pol, checker)
    }

    /// Get the element type of the given type.
    pub fn element_of(&self, pol: bool, checker: &mut impl ApplyChecker) {
        self.apply(SigSurfaceKind::ArrayOrDict, &EMPTY_ARGS, pol, checker)
    }

    fn apply(
        &self,
        surface: SigSurfaceKind,
        args: &Interned<ArgsTy>,
        pol: bool,
        checker: &mut impl ApplyChecker,
    ) {
        let mut worker = ApplySigChecker(checker, args);
        worker.ty(self, surface, pol);
    }
}

pub struct ApplySigChecker<'a, T>(&'a mut T, &'a Interned<ArgsTy>);

impl<'a, T: ApplyChecker> ApplySigChecker<'a, T> {
    fn ty(&mut self, ty: &Ty, surface: SigSurfaceKind, pol: bool) {
        ty.sig_surface(pol, surface, self)
    }
}

impl<'a, T: ApplyChecker> SigChecker for ApplySigChecker<'a, T> {
    fn check(&mut self, cano_sig: Sig, ctx: &mut super::SigCheckContext, pol: bool) -> Option<()> {
        // Bind the arguments to the canonical signature.
        let partial_sig = if ctx.args.is_empty() {
            cano_sig
        } else {
            Sig::With {
                sig: &cano_sig,
                withs: &ctx.args,
                at: &ctx.at,
            }
        };
        self.0.apply(partial_sig, self.1, pol);
        Some(())
    }

    fn check_var(&mut self, _var: &Interned<TypeVar>, _pol: bool) -> Option<TypeBounds> {
        self.0.bound_of_var(_var, _pol)
    }
}
