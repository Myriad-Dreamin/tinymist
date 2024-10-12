//! Type checking at select site

use typst::syntax::Span;

use crate::analysis::SelectChecker;
use crate::analysis::Ty;

use super::*;
use crate::adt::interner::Interned;

pub struct SelectFieldChecker<'a, 'b, 'w> {
    pub(super) base: &'a mut TypeChecker<'b, 'w>,
    pub select_site: Span,
    pub key: &'a Interned<str>,
    pub resultant: Vec<Ty>,
}

impl<'a, 'b, 'w> SelectChecker for SelectFieldChecker<'a, 'b, 'w> {
    fn bound_of_var(
        &mut self,
        var: &Interned<super::TypeVar>,
        _pol: bool,
    ) -> Option<super::TypeBounds> {
        self.base
            .info
            .vars
            .get(&var.def)
            .map(|v| v.bounds.bounds().read().clone())
    }

    fn select(&mut self, iface: Iface, key: &Interned<str>, pol: bool) {
        println!("selecting field: {iface:?} {key:?}");
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
