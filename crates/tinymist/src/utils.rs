use core::fmt;

use lsp_server::{ErrorCode, ResponseError};

#[derive(Clone)]
pub struct Derived<T>(pub T);

impl<T> fmt::Debug for Derived<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("..")
    }
}

/// Get a parsed command argument.
/// Return `INVALID_PARAMS` when no arg or parse failed.
macro_rules! get_arg {
    ($args:ident[$idx:expr] as $ty:ty) => {{
        let arg = $args.get_mut($idx);
        let arg = arg.and_then(|x| from_value::<$ty>(x.take()).ok());
        match arg {
            Some(v) => v,
            None => {
                let msg = concat!("expect ", stringify!($ty), "at args[", $idx, "]");
                return Err(invalid_params(msg));
            }
        }
    }};
}
pub(crate) use get_arg;

/// Get a parsed command argument or default if no arg.
/// Return `INVALID_PARAMS` when parse failed.
macro_rules! get_arg_or_default {
    ($args:ident[$idx:expr] as $ty:ty) => {{
        if $idx >= $args.len() {
            Default::default()
        } else {
            get_arg!($args[$idx] as $ty)
        }
    }};
}
pub(crate) use get_arg_or_default;

pub fn z_internal_error(msg: typst_ts_core::Error) -> ResponseError {
    ResponseError {
        code: ErrorCode::InternalError as i32,
        message: format!("internal: {msg:?}"),
        data: None,
    }
}

pub fn try_<T>(f: impl FnOnce() -> Option<T>) -> Option<T> {
    f()
}

pub fn try_or<T>(f: impl FnOnce() -> Option<T>, default: T) -> T {
    f().unwrap_or(default)
}

pub fn try_or_default<T: Default>(f: impl FnOnce() -> Option<T>) -> T {
    f().unwrap_or_default()
}
