use core::fmt;
use std::{borrow::Cow, ops::Deref};

/// An error that can occur during the conversion process.
pub struct Error(Box<Repr>);

enum Repr {
    /// Just a message.
    Msg(Cow<'static, str>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.deref() {
            Repr::Msg(s) => write!(f, "{s}"),
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <Self as fmt::Display>::fmt(self, f)
    }
}

impl<T> From<T> for Error
where
    T: Into<Cow<'static, str>>,
{
    fn from(s: T) -> Self {
        Error(Box::new(Repr::Msg(s.into())))
    }
}
