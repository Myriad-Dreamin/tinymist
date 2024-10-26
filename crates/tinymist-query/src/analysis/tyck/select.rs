//! Type checking at select site

use super::*;
use crate::analysis::SelectChecker;

#[derive(BindTyCtx)]
#[bind(base)]
pub struct SelectFieldChecker<'a, 'b> {
    pub(super) base: &'a mut TypeChecker<'b>,
    pub select_site: Span,
    pub resultant: Vec<Ty>,
}

impl<'a, 'b> SelectChecker for SelectFieldChecker<'a, 'b> {
    fn select(&mut self, iface: Iface, key: &Interned<str>, pol: bool) {
        log::debug!("selecting field: {iface:?} {key:?}");
        let _ = pol;

        let ins = iface.ty();
        if let Some(ins) = ins {
            self.base.info.witness_at_least(self.select_site, ins);
        }

        let Some(IfaceShape { iface }) = iface.shape(self.base) else {
            return;
        };

        let res = iface.field_by_name(key);
        log::debug!("selecting field real: {key:?} -> {res:?}");
        if let Some(res) = res {
            self.resultant.push(res.clone());
        }
    }
}
