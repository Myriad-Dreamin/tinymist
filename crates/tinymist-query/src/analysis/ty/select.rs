//! Type checking at select site

use typst::syntax::Span;

use crate::analysis::SelectChecker;
use crate::analysis::Ty;

use super::*;
use crate::adt::interner::Interned;

#[derive(BindTyCtx)]
#[bind(base)]
pub struct SelectFieldChecker<'a, 'b, 'w> {
    pub(super) base: &'a mut TypeChecker<'b, 'w>,
    pub select_site: Span,
    pub key: &'a Interned<str>,
    pub resultant: Vec<Ty>,
}

impl<'a, 'b, 'w> SelectChecker for SelectFieldChecker<'a, 'b, 'w> {
    fn select(&mut self, iface: Iface, key: &Interned<str>, pol: bool) {
        log::debug!("selecting field: {iface:?} {key:?}");
        let _ = pol;

        let ins = iface.ty();
        if let Some(ins) = ins {
            self.base.info.witness_at_least(self.select_site, ins);
        }

        let Some(IfaceShape { iface }) = iface.shape(Some(self.base.ctx)) else {
            return;
        };

        let res = iface.field_by_name(self.key);
        if let Some(res) = res {
            self.resultant.push(res.clone());
        }
    }
}
