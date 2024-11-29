//! Type checking at select site

use super::*;
use crate::analysis::SelectChecker;

#[derive(BindTyCtx)]
#[bind(base)]
pub struct SelectFieldChecker<'a, 'b> {
    pub(super) base: &'a mut TypeChecker<'b>,
    pub resultant: Vec<Ty>,
}

impl SelectChecker for SelectFieldChecker<'_, '_> {
    fn select(&mut self, iface: Iface, key: &Interned<str>, pol: bool) {
        crate::log_debug_ct!("selecting field: {iface:?} {key:?}");
        let _ = pol;

        let Some(res) = iface.select(self.base, key) else {
            return;
        };

        crate::log_debug_ct!("selecting field real: {key:?} -> {res:?}");
        self.resultant.push(res.clone());
    }
}
