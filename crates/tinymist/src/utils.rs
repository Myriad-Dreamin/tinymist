use core::fmt;

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
        let arg = match arg {
            Some(v) => v,
            None => {
                let msg = concat!("expect ", stringify!($ty), " at args[", $idx, "]");
                return Err(invalid_params(msg));
            }
        };
        match from_value::<$ty>(arg.take()) {
            Ok(v) => v,
            Err(err) => {
                let msg = concat!("expect ", stringify!($ty), " at args[", $idx, "], error: ");
                return Err(invalid_params(format!("{}{}", msg, err)));
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

pub fn try_<T>(f: impl FnOnce() -> Option<T>) -> Option<T> {
    f()
}

pub fn try_or<T>(f: impl FnOnce() -> Option<T>, default: T) -> T {
    f().unwrap_or(default)
}

pub fn try_or_default<T: Default>(f: impl FnOnce() -> Option<T>) -> T {
    f().unwrap_or_default()
}
