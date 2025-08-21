use std::sync::LazyLock;

use super::{Sig, SigChecker, SigSurfaceKind, TyCtx};
use crate::ty::prelude::*;

/// A trait for checking the application of a signature.
pub trait ApplyChecker: TyCtx {
    /// Applies a signature to the given arguments.
    fn apply(&mut self, sig: Sig, arguments: &Interned<ArgsTy>, pol: bool);
}

/// The empty arguments type.
static EMPTY_ARGS: LazyLock<Interned<ArgsTy>> = LazyLock::new(|| ArgsTy::default().into());

impl Ty {
    /// Calls the given type with the given arguments.
    pub fn call(&self, args: &Interned<ArgsTy>, pol: bool, c: &mut impl ApplyChecker) {
        ApplySigChecker(c, args).ty(self, SigSurfaceKind::Call, pol);
    }

    /// Gets the tuple element type of the given type.
    pub fn tuple_element_of(&self, pol: bool, c: &mut impl ApplyChecker) {
        ApplySigChecker(c, &EMPTY_ARGS).ty(self, SigSurfaceKind::Array, pol);
    }

    /// Get the element type of the given type.
    pub fn element_of(&self, pol: bool, c: &mut impl ApplyChecker) {
        ApplySigChecker(c, &EMPTY_ARGS).ty(self, SigSurfaceKind::ArrayOrDict, pol);
    }
}

/// A checker for applying a signature to a type.
#[derive(BindTyCtx)]
#[bind(0)]
pub struct ApplySigChecker<'a, T: ApplyChecker>(&'a mut T, &'a Interned<ArgsTy>);

impl<T: ApplyChecker> ApplySigChecker<'_, T> {
    /// Applies a signature to a type.
    fn ty(&mut self, ty: &Ty, surface: SigSurfaceKind, pol: bool) {
        ty.sig_surface(pol, surface, self)
    }
}

impl<T: ApplyChecker> SigChecker for ApplySigChecker<'_, T> {
    /// Checks a signature against a context.
    fn check(&mut self, cano_sig: Sig, ctx: &mut super::SigCheckContext, pol: bool) -> Option<()> {
        let (cano_sig, is_partialize) = match cano_sig {
            Sig::Partialize(sig) => (*sig, true),
            sig => (sig, false),
        };
        // Binds the arguments to the canonical signature.
        let partial_sig = if ctx.args.is_empty() {
            cano_sig
        } else {
            Sig::With {
                sig: &cano_sig,
                withs: &ctx.args,
                at: &ctx.at,
            }
        };
        let partial_sig = if is_partialize {
            Sig::Partialize(&partial_sig)
        } else {
            partial_sig
        };

        self.0.apply(partial_sig, self.1, pol);
        Some(())
    }
}
