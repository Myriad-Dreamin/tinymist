pub mod lsp;
pub mod lsp_cmd;
pub mod lsp_init;

pub mod compile;
pub mod compile_cmd;
pub mod compile_init;

pub mod preview;

use serde_json::{from_value, Value as JsonValue};

// type AnySchedulableResponse = LspResult<Pin<Box<dyn
// std::future::Future<Output = LspResult<JsonValue>> + Send>>>;
type AnySchedulableResponse = LspResult<JsonValue>;

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
use get_arg;

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
use get_arg_or_default;

use crate::LspResult;
