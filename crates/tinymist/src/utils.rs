use core::fmt;
use std::thread;

use lsp_server::{ErrorCode, ResponseError};
use tokio::sync::oneshot;
use typst_ts_core::error::prelude::*;
use typst_ts_core::Error;

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

pub fn threaded_receive<T: Send>(f: oneshot::Receiver<T>) -> Result<T, Error> {
    // get current async handle
    if let Ok(e) = tokio::runtime::Handle::try_current() {
        // todo: remove blocking
        return thread::scope(|s| {
            s.spawn(move || {
                e.block_on(f)
                    .map_err(map_string_err("failed to receive data"))
            })
            .join()
            .map_err(|_| error_once!("failed to join"))?
        });
    }

    f.blocking_recv()
        .map_err(map_string_err("failed to recv from receive data"))
}

#[cfg(test)]
mod tests {
    fn do_receive() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        tx.send(1).unwrap();
        let res = super::threaded_receive(rx).unwrap();
        assert_eq!(res, 1);
    }
    #[test]
    fn test_sync() {
        do_receive();
    }
    #[test]
    fn test_single_threaded() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async { do_receive() });
    }
    #[test]
    fn test_multiple_threaded() {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async { do_receive() });
    }
}
