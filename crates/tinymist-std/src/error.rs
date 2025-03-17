//! Error handling utilities for the `tinymist` crate.

use core::fmt;

use ecow::EcoString;
use serde::{Deserialize, Serialize};
#[cfg(feature = "typst")]
use typst::diag::SourceDiagnostic;

use lsp_types::Range as LspRange;

/// The severity of a diagnostic message, following the LSP specification.
#[derive(serde_repr::Serialize_repr, serde_repr::Deserialize_repr, Debug, Clone)]
#[repr(u8)]
pub enum DiagSeverity {
    /// An error message.
    Error = 1,
    /// A warning message.
    Warning = 2,
    /// An information message.
    Information = 3,
    /// A hint message.
    Hint = 4,
}

impl fmt::Display for DiagSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagSeverity::Error => write!(f, "error"),
            DiagSeverity::Warning => write!(f, "warning"),
            DiagSeverity::Information => write!(f, "information"),
            DiagSeverity::Hint => write!(f, "hint"),
        }
    }
}

/// <https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#diagnostic>
/// The `owner` and `source` fields are not included in the struct, but they
/// could be added to `ErrorImpl::arguments`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagMessage {
    /// The typst package specifier.
    pub package: String,
    /// The file path relative to the root of the workspace or the package.
    pub path: String,
    /// The diagnostic message.
    pub message: EcoString,
    /// The severity of the diagnostic message.
    pub severity: DiagSeverity,
    /// The char range in the file. The position encoding must be negotiated.
    pub range: Option<LspRange>,
}

impl DiagMessage {}

/// ALl kind of errors that can occur in the `tinymist` crate.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ErrKind {
    /// No message.
    None,
    /// A string message.
    Msg(EcoString),
    /// A source diagnostic message.
    #[cfg(feature = "typst")]
    RawDiag(ecow::EcoVec<SourceDiagnostic>),
    /// A source diagnostic message.
    Diag(Box<DiagMessage>),
    /// An inner error.
    Inner(Error),
}

/// A trait to convert an error kind into an error kind.
pub trait ErrKindExt {
    /// Convert the error kind into an error kind.
    fn to_error_kind(self) -> ErrKind;
}

impl ErrKindExt for ErrKind {
    fn to_error_kind(self) -> Self {
        self
    }
}

impl ErrKindExt for std::io::Error {
    fn to_error_kind(self) -> ErrKind {
        ErrKind::Msg(self.to_string().into())
    }
}

impl ErrKindExt for std::str::Utf8Error {
    fn to_error_kind(self) -> ErrKind {
        ErrKind::Msg(self.to_string().into())
    }
}

impl ErrKindExt for String {
    fn to_error_kind(self) -> ErrKind {
        ErrKind::Msg(self.into())
    }
}

impl ErrKindExt for &str {
    fn to_error_kind(self) -> ErrKind {
        ErrKind::Msg(self.into())
    }
}

impl ErrKindExt for &String {
    fn to_error_kind(self) -> ErrKind {
        ErrKind::Msg(self.into())
    }
}

impl ErrKindExt for EcoString {
    fn to_error_kind(self) -> ErrKind {
        ErrKind::Msg(self)
    }
}

impl ErrKindExt for &dyn std::fmt::Display {
    fn to_error_kind(self) -> ErrKind {
        ErrKind::Msg(self.to_string().into())
    }
}

impl ErrKindExt for serde_json::Error {
    fn to_error_kind(self) -> ErrKind {
        ErrKind::Msg(self.to_string().into())
    }
}

impl ErrKindExt for anyhow::Error {
    fn to_error_kind(self) -> ErrKind {
        ErrKind::Msg(self.to_string().into())
    }
}

impl ErrKindExt for Error {
    fn to_error_kind(self) -> ErrKind {
        ErrKind::Msg(self.to_string().into())
    }
}

/// The internal error implementation.
#[derive(Debug, Clone)]
pub struct ErrorImpl {
    /// A static error identifier.
    loc: &'static str,
    /// The kind of error.
    kind: ErrKind,
    /// Additional extractable arguments for the error.
    args: Option<Box<[(&'static str, String)]>>,
}

/// This type represents all possible errors that can occur in typst.ts
#[derive(Clone)]
pub struct Error {
    /// This `Box` allows us to keep the size of `Error` as small as possible. A
    /// larger `Error` type was substantially slower due to all the functions
    /// that pass around `Result<T, Error>`.
    err: Box<ErrorImpl>,
}

impl Error {
    /// Creates a new error.
    pub fn new(
        loc: &'static str,
        kind: ErrKind,
        args: Option<Box<[(&'static str, String)]>>,
    ) -> Self {
        Self {
            err: Box::new(ErrorImpl { loc, kind, args }),
        }
    }

    /// Returns the location of the error.
    pub fn loc(&self) -> &'static str {
        self.err.loc
    }

    /// Returns the kind of the error.
    pub fn kind(&self) -> &ErrKind {
        &self.err.kind
    }

    /// Returns the arguments of the error.
    pub fn arguments(&self) -> &[(&'static str, String)] {
        self.err.args.as_deref().unwrap_or_default()
    }

    /// Returns the diagnostics attach to the error.
    #[cfg(feature = "typst")]
    pub fn diagnostics(&self) -> Option<&[SourceDiagnostic]> {
        match &self.err.kind {
            ErrKind::RawDiag(diag) => Some(diag),
            _ => None,
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as fmt::Display>::fmt(self, f)
    }
}

macro_rules! write_with_args {
    ($f:expr, $args:expr, $fmt:expr  $(, $arg:expr)*) => {
        if let Some(args) = $args.as_ref() {
            write!($f, "{}, with {:?}", format_args!($fmt $(, $arg)*), args)
        } else {
            write!($f, $fmt $(, $arg)*)
        }
    };
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let err = &self.err;

        if err.loc.is_empty() {
            match &err.kind {
                ErrKind::Msg(msg) => {
                    if msg.is_empty() {
                        write_with_args!(f, err.args, "{}", err.loc)
                    } else {
                        write_with_args!(f, err.args, "{}: {msg}", err.loc)
                    }
                }
                #[cfg(feature = "typst")]
                ErrKind::RawDiag(diag) => {
                    write_with_args!(f, err.args, "{diag:?}")
                }
                ErrKind::Diag(diag) => {
                    write_with_args!(f, err.args, "{}", diag.message)
                }
                ErrKind::Inner(e) => write_with_args!(f, err.args, "{e}"),
                ErrKind::None => write_with_args!(f, err.args, "unknwon error"),
            }
        } else {
            match &err.kind {
                ErrKind::Msg(msg) => {
                    if msg.is_empty() {
                        write_with_args!(f, err.args, "{}", err.loc)
                    } else {
                        write_with_args!(f, err.args, "{}: {msg}", err.loc)
                    }
                }
                #[cfg(feature = "typst")]
                ErrKind::RawDiag(diag) => {
                    write_with_args!(f, err.args, "{}: {diag:?}", err.loc)
                }
                ErrKind::Diag(diag) => {
                    write_with_args!(f, err.args, "{}: {}", err.loc, diag.message)
                }
                ErrKind::Inner(e) => write_with_args!(f, err.args, "{}: {}", err.loc, e),
                ErrKind::None => write_with_args!(f, err.args, "{}", err.loc),
            }
        }
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::new("", e.to_string().to_error_kind(), None)
    }
}

#[cfg(feature = "typst")]
impl From<ecow::EcoVec<SourceDiagnostic>> for Error {
    fn from(e: ecow::EcoVec<SourceDiagnostic>) -> Self {
        Error::new("", ErrKind::RawDiag(e), None)
    }
}

impl std::error::Error for Error {}

#[cfg(feature = "web")]
impl ErrKindExt for wasm_bindgen::JsValue {
    fn to_error_kind(self) -> ErrKind {
        ErrKind::Msg(ecow::eco_format!("{self:?}"))
    }
}

#[cfg(feature = "web")]
impl From<Error> for wasm_bindgen::JsValue {
    fn from(e: Error) -> Self {
        js_sys::Error::new(&e.to_string()).into()
    }
}

#[cfg(feature = "web")]
impl From<&Error> for wasm_bindgen::JsValue {
    fn from(e: &Error) -> Self {
        js_sys::Error::new(&e.to_string()).into()
    }
}

/// The result type used in the `tinymist` crate.
pub type Result<T, Err = Error> = std::result::Result<T, Err>;

/// A trait to add context to a result.
pub trait IgnoreLogging<T>: Sized {
    /// Log an error message and return `None`.
    fn log_error(self, msg: &str) -> Option<T>;
    /// Log an error message and return `None`.
    fn log_error_with(self, f: impl FnOnce() -> String) -> Option<T>;
}

impl<T, E: std::fmt::Display> IgnoreLogging<T> for Result<T, E> {
    fn log_error(self, msg: &str) -> Option<T> {
        self.inspect_err(|e| log::error!("{msg}: {e}")).ok()
    }

    fn log_error_with(self, f: impl FnOnce() -> String) -> Option<T> {
        self.inspect_err(|e| log::error!("{}: {e}", f())).ok()
    }
}

/// A trait to add context to a result.
pub trait WithContext<T>: Sized {
    /// Add a context to the result.
    fn context(self, loc: &'static str) -> Result<T>;

    /// Add a context to the result with additional arguments.
    fn with_context<F>(self, loc: &'static str, f: F) -> Result<T>
    where
        F: FnOnce() -> Option<Box<[(&'static str, String)]>>;
}

impl<T, E: ErrKindExt> WithContext<T> for Result<T, E> {
    fn context(self, loc: &'static str) -> Result<T> {
        self.map_err(|e| Error::new(loc, e.to_error_kind(), None))
    }

    fn with_context<F>(self, loc: &'static str, f: F) -> Result<T>
    where
        F: FnOnce() -> Option<Box<[(&'static str, String)]>>,
    {
        self.map_err(|e| Error::new(loc, e.to_error_kind(), f()))
    }
}

impl<T> WithContext<T> for Option<T> {
    fn context(self, loc: &'static str) -> Result<T> {
        self.ok_or_else(|| Error::new(loc, ErrKind::None, None))
    }

    fn with_context<F>(self, loc: &'static str, f: F) -> Result<T>
    where
        F: FnOnce() -> Option<Box<[(&'static str, String)]>>,
    {
        self.ok_or_else(|| Error::new(loc, ErrKind::None, f()))
    }
}

/// A trait to add context to a result without a specific error type.
pub trait WithContextUntyped<T>: Sized {
    /// Add a context to the result.
    fn context_ut(self, loc: &'static str) -> Result<T>;

    /// Add a context to the result with additional arguments.
    fn with_context_ut<F>(self, loc: &'static str, f: F) -> Result<T>
    where
        F: FnOnce() -> Option<Box<[(&'static str, String)]>>;
}

impl<T, E: std::fmt::Display> WithContextUntyped<T> for Result<T, E> {
    fn context_ut(self, loc: &'static str) -> Result<T> {
        self.map_err(|e| Error::new(loc, ErrKind::Msg(ecow::eco_format!("{e}")), None))
    }

    fn with_context_ut<F>(self, loc: &'static str, f: F) -> Result<T>
    where
        F: FnOnce() -> Option<Box<[(&'static str, String)]>>,
    {
        self.map_err(|e| Error::new(loc, ErrKind::Msg(ecow::eco_format!("{e}")), f()))
    }
}

/// The error prelude.
pub mod prelude {
    #![allow(missing_docs)]

    use super::ErrKindExt;
    use crate::Error;

    pub use super::{IgnoreLogging, WithContext, WithContextUntyped};
    pub use crate::{bail, Result};

    pub fn map_string_err<T: ToString>(loc: &'static str) -> impl Fn(T) -> Error {
        move |e| Error::new(loc, e.to_string().to_error_kind(), None)
    }

    pub fn map_into_err<S: ErrKindExt, T: Into<S>>(loc: &'static str) -> impl Fn(T) -> Error {
        move |e| Error::new(loc, e.into().to_error_kind(), None)
    }

    pub fn map_err<T: ErrKindExt>(loc: &'static str) -> impl Fn(T) -> Error {
        move |e| Error::new(loc, e.to_error_kind(), None)
    }

    pub fn wrap_err(loc: &'static str) -> impl Fn(Error) -> Error {
        move |e| Error::new(loc, crate::ErrKind::Inner(e), None)
    }

    pub fn map_string_err_with_args<
        T: ToString,
        Args: IntoIterator<Item = (&'static str, String)>,
    >(
        loc: &'static str,
        args: Args,
    ) -> impl FnOnce(T) -> Error {
        move |e| {
            Error::new(
                loc,
                e.to_string().to_error_kind(),
                Some(args.into_iter().collect::<Vec<_>>().into_boxed_slice()),
            )
        }
    }

    pub fn map_into_err_with_args<
        S: ErrKindExt,
        T: Into<S>,
        Args: IntoIterator<Item = (&'static str, String)>,
    >(
        loc: &'static str,
        args: Args,
    ) -> impl FnOnce(T) -> Error {
        move |e| {
            Error::new(
                loc,
                e.into().to_error_kind(),
                Some(args.into_iter().collect::<Vec<_>>().into_boxed_slice()),
            )
        }
    }

    pub fn map_err_with_args<T: ErrKindExt, Args: IntoIterator<Item = (&'static str, String)>>(
        loc: &'static str,
        args: Args,
    ) -> impl FnOnce(T) -> Error {
        move |e| {
            Error::new(
                loc,
                e.to_error_kind(),
                Some(args.into_iter().collect::<Vec<_>>().into_boxed_slice()),
            )
        }
    }

    pub fn wrap_err_with_args<Args: IntoIterator<Item = (&'static str, String)>>(
        loc: &'static str,
        args: Args,
    ) -> impl FnOnce(Error) -> Error {
        move |e| {
            Error::new(
                loc,
                crate::ErrKind::Inner(e),
                Some(args.into_iter().collect::<Vec<_>>().into_boxed_slice()),
            )
        }
    }

    pub fn _error_once(loc: &'static str, args: Box<[(&'static str, String)]>) -> Error {
        Error::new(loc, crate::ErrKind::None, Some(args))
    }

    pub fn _msg(loc: &'static str, msg: EcoString) -> Error {
        Error::new(loc, crate::ErrKind::Msg(msg), None)
    }

    pub use ecow::eco_format as _eco_format;

    #[macro_export]
    macro_rules! bail {
        ($($arg:tt)+) => {{
            let args = $crate::error::prelude::_eco_format!($($arg)+);
            return Err($crate::error::prelude::_msg(concat!(file!(), ":", line!(), ":", column!()), args))
        }};
    }

    #[macro_export]
    macro_rules! error_once {
        ($loc:expr, $($arg_key:ident: $arg:expr),+ $(,)?) => {
            $crate::error::prelude::_error_once($loc, Box::new([$((stringify!($arg_key), $arg.to_string())),+]))
        };
        ($loc:expr $(,)?) => {
            $crate::error::prelude::_error_once($loc, Box::new([]))
        };
    }

    #[macro_export]
    macro_rules! error_once_map {
        ($loc:expr, $($arg_key:ident: $arg:expr),+ $(,)?) => {
            $crate::error::prelude::map_err_with_args($loc, [$((stringify!($arg_key), $arg.to_string())),+])
        };
        ($loc:expr $(,)?) => {
            $crate::error::prelude::map_err($loc)
        };
    }

    #[macro_export]
    macro_rules! error_once_map_string {
        ($loc:expr, $($arg_key:ident: $arg:expr),+ $(,)?) => {
            $crate::error::prelude::map_string_err_with_args($loc, [$((stringify!($arg_key), $arg.to_string())),+])
        };
        ($loc:expr $(,)?) => {
            $crate::error::prelude::map_string_err($loc)
        };
    }

    use ecow::EcoString;
    pub use error_once;
    pub use error_once_map;
    pub use error_once_map_string;
}

#[test]
fn test_send() {
    fn is_send<T: Send>() {}
    is_send::<Error>();
}
