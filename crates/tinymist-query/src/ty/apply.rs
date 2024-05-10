use typst::foundations::Func;

use crate::{adt::interner::Interned, ty::def::*};

use super::{Sig, SigChecker, SigSurfaceKind};

pub trait ApplyChecker {
    fn call(&mut self, sig: Sig, arguments: &Interned<ArgsTy>, pol: bool);

    fn func_sig_of(&mut self, func: &Func) -> Option<Interned<SigTy>>;

    fn bound_of_var(&mut self, _var: &Interned<TypeVar>, _pol: bool) -> Option<TypeBounds> {
        None
    }
}

impl Ty {
    pub fn call(&self, args: &Interned<ArgsTy>, pol: bool, checker: &mut impl ApplyChecker) {
        self.apply(SigSurfaceKind::Call, args, pol, checker)
    }

    // pub fn element_of(&self, pol: bool, checker: &mut impl ApplyChecker) {
    //     static EMPTY_ARGS: Lazy<Interned<ArgsTy>> =
    //       Lazy::new(|| Interned::new(ArgsTy::default()));

    //     self.apply(SigSurfaceKind::ArrayOrDict, &EMPTY_ARGS, pol, checker)
    // }

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
        let cano_sig = match cano_sig {
            Sig::Partialize(Sig::Value { val, .. }) => {
                let sig = self.0.func_sig_of(val)?;
                return self.check(Sig::Partialize(&Sig::Type(&sig)), ctx, pol);
            }
            Sig::Value { val, .. } => {
                let sig = self.0.func_sig_of(val)?;
                return self.check(Sig::Type(&sig), ctx, pol);
            }
            sig => sig,
        };

        let args = &ctx.args;
        let partial_sig = if args.is_empty() {
            cano_sig
        } else {
            Sig::With {
                sig: &cano_sig,
                withs: args,
                at: &ctx.at,
            }
        };

        self.0.call(partial_sig, self.1, pol);
        Some(())
    }

    fn check_var(&mut self, _var: &Interned<TypeVar>, _pol: bool) -> Option<TypeBounds> {
        self.0.bound_of_var(_var, _pol)
    }
}
