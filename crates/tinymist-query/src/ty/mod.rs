mod def;
mod visit_sig;
pub(crate) use def::*;
pub(crate) use visit_sig::*;

use typst::foundations::Func;

use crate::adt::interner::Interned;

pub enum Sig<'a> {
    ArrayCons(&'a TyRef),
    DictCons(&'a Interned<RecordTy>),
    Type(&'a Interned<SigTy>),
    Value(&'a Func),
}
