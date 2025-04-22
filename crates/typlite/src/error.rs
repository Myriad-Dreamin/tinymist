use core::fmt;
use std::{borrow::Cow, ops::Deref};

/// An error that can occur during the conversion process.
#[derive(Clone)]
pub struct Error(Box<Repr>);

#[derive(Clone)]
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

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(Box::new(Repr::Msg(e.to_string().into())))
    }
}

impl From<fmt::Error> for Error {
    fn from(e: fmt::Error) -> Self {
        Error(Box::new(Repr::Msg(e.to_string().into())))
    }
}

impl From<&'static str> for Error {
    fn from(s: &'static str) -> Self {
        Error(Box::new(Repr::Msg(s.into())))
    }
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error(Box::new(Repr::Msg(s.into())))
    }
}

impl From<Cow<'static, str>> for Error {
    fn from(s: Cow<'static, str>) -> Self {
        Error(Box::new(Repr::Msg(s)))
    }
}
