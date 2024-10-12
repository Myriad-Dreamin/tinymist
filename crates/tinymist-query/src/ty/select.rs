use crate::{adt::interner::Interned, ty::def::*};

use super::{Iface, IfaceChecker};

pub trait SelectChecker {
    fn select(&mut self, sig: Iface, key: &Interned<str>, pol: bool);

    fn bound_of_var(&mut self, _var: &Interned<TypeVar>, _pol: bool) -> Option<TypeBounds> {
        None
    }
}

impl Ty {
    /// Select the given type with the given key.
    pub fn select(&self, key: &Interned<str>, pol: bool, checker: &mut impl SelectChecker) {
        SelectKeyChecker(checker, key).ty(self, pol);
    }
}

pub struct SelectKeyChecker<'a, T>(&'a mut T, &'a Interned<str>);

impl<'a, T: SelectChecker> SelectKeyChecker<'a, T> {
    fn ty(&mut self, ty: &Ty, pol: bool) {
        ty.iface_surface(pol, self)
    }
}

impl<'a, T: SelectChecker> IfaceChecker for SelectKeyChecker<'a, T> {
    fn check(
        &mut self,
        iface: Iface,
        _ctx: &mut super::IfaceCheckContext,
        pol: bool,
    ) -> Option<()> {
        self.0.select(iface, self.1, pol);
        Some(())
    }

    fn check_var(&mut self, _var: &Interned<TypeVar>, _pol: bool) -> Option<TypeBounds> {
        self.0.bound_of_var(_var, _pol)
    }
}
