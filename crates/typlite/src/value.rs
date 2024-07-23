use crate::library::ArgGetter;
use crate::*;

pub type RawFunc = fn(ArgGetter) -> Result<Value>;

#[derive(Debug)]
pub enum Value {
    RawFunc(RawFunc),
    Content(EcoString),
}

impl From<RawFunc> for Value {
    fn from(func: RawFunc) -> Self {
        Self::RawFunc(func)
    }
}
