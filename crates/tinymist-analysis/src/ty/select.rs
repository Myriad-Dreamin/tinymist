use super::{Iface, IfaceChecker};
use crate::ty::def::*;

/// A trait to check the select of a type.
pub trait SelectChecker: TyCtx {
    /// Checks the select of the given type.
    fn select(&mut self, sig: Iface, key: &Interned<str>, pol: bool);
}

impl Ty {
    /// Selects the given type with the given key.
    pub fn select(&self, key: &Interned<str>, pol: bool, checker: &mut impl SelectChecker) {
        SelectKeyChecker(checker, key).ty(self, pol);
    }
}

/// A checker to check the select of a type.
#[derive(BindTyCtx)]
#[bind(0)]
pub struct SelectKeyChecker<'a, T: TyCtx>(&'a mut T, &'a Interned<str>);

/// A driver to check the select of a type.
impl<T: SelectChecker> SelectKeyChecker<'_, T> {
    fn ty(&mut self, ty: &Ty, pol: bool) {
        ty.iface_surface(pol, self)
    }
}

/// A checker to check the select of a type.
impl<T: SelectChecker> IfaceChecker for SelectKeyChecker<'_, T> {
    fn check(
        &mut self,
        iface: Iface,
        _ctx: &mut super::IfaceCheckContext,
        pol: bool,
    ) -> Option<()> {
        self.0.select(iface, self.1, pol);
        Some(())
    }
}
