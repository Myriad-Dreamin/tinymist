//! Type checking at select site

use super::*;
use crate::analysis::SelectChecker;

#[derive(BindTyCtx)]
#[bind(base)]
pub struct SelectFieldChecker<'a, 'b> {
    pub(super) base: &'a mut TypeChecker<'b>,
    pub resultant: Vec<Ty>,
}

impl<'a, 'b> SelectChecker for SelectFieldChecker<'a, 'b> {
    fn select(&mut self, iface: Iface, key: &Interned<str>, pol: bool) {
        log::debug!("selecting field: {iface:?} {key:?}");
        let _ = pol;

        let Some(res) = iface.select(self.base, key) else {
            return;
        };

        log::debug!("selecting field real: {key:?} -> {res:?}");
        self.resultant.push(res.clone());
    }
}
