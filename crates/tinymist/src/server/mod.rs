pub mod lsp;
pub mod lsp_cmd;
pub mod lsp_init;

pub mod compile;
pub mod compile_cmd;
pub mod compile_init;

#[cfg(feature = "preview")]
pub mod preview;

use std::collections::HashMap;

use lsp_server::RequestId;
use reflexo::ImmutPath;
use serde_json::{from_value, Value as JsonValue};

/// Returns Ok(Some()) -> Already responded
/// Returns Ok(None) -> Need to respond none
/// Returns Err(..) -> Need to respond error
type LspRawPureHandler<S, T> = fn(srv: &mut S, args: T) -> LspResult<()>;
type LspRawHandler<S, T> = fn(srv: &mut S, req_id: RequestId, args: T) -> LspResult<Option<()>>;
type ExecuteCmdMap<S> = HashMap<&'static str, LspRawHandler<S, Vec<JsonValue>>>;
type RegularCmdMap<S> = HashMap<&'static str, LspRawHandler<S, JsonValue>>;
// type LspMethod<Res> = fn(srv: &mut LanguageState, args: JsonValue) ->
// LspResult<Res>;

// type LspHandler<Req, Res> = fn(srv: &mut LanguageState, args:
// Req) -> LspResult<Res>;
type NotifyCmdMap<S> = HashMap<&'static str, LspRawPureHandler<S, JsonValue>>;
type ResourceMap<S> = HashMap<ImmutPath, LspRawHandler<S, Vec<JsonValue>>>;

use crate::ScheduledResult;
type SchedulableResponse<T> = LspResponseFuture<LspResult<T>>;
type AnySchedulableResponse = SchedulableResponse<JsonValue>;
// type AnySchedulableResponse = LspResult<JsonValue>;

macro_rules! request_fn_ {
    ($desc: ty, $s: ident::$method: ident) => {
        (<$desc>::METHOD, {
            const E: LspRawHandler<$s, JsonValue> = |this, req_id, req| {
                let req: <$desc as lsp_types::request::Request>::Params =
                    serde_json::from_value(req).unwrap(); // todo: soft unwrap
                this.$method(req_id, req)
            };
            E
        })
    };
}
use request_fn_;

macro_rules! request_fn {
    ($desc: ty, $s: ident::$method: ident) => {
        (<$desc>::METHOD, {
            const E: LspRawHandler<$s, JsonValue> = |this, req_id, req| {
                let req: <$desc as lsp_types::request::Request>::Params =
                    serde_json::from_value(req).unwrap(); // todo: soft unwrap
                let res = this.$method(req);
                this.schedule(req_id, res)
            };
            E
        })
    };
}
use request_fn;

macro_rules! exec_fn_ {
    ($key: expr, $s: ident::$method: ident) => {
        ($key, {
            const E: LspRawHandler<$s, Vec<JsonValue>> =
                |this, req_id, req| this.$method(req_id, req);
            E
        })
    };
}
use exec_fn_;

// let result = handler(self, req.id.clone(), req.params);
// match result {
//     Ok(Some(())) => {}
//     _ => self.client.respond(result_to_response(req.id, result)),
// }

macro_rules! exec_fn {
    ($key: expr, $s: ident::$method: ident) => {
        ($key, {
            const E: LspRawHandler<$s, Vec<JsonValue>> = |this, req_id, req| {
                let res = this.$method(req);
                this.schedule(req_id, res)
            };
            E
        })
    };
}
use exec_fn;

macro_rules! resource_fn {
    ($s: ident::$method: ident) => {{
        const E: LspRawHandler<$s, Vec<JsonValue>> = |this, req_id, req| {
            let res = this.$method(req);
            this.schedule(req_id, res)
        };
        E
    }};
}
use resource_fn;

macro_rules! notify_fn {
    ($desc: ty, $s: ident::$method: ident) => {
        (<$desc>::METHOD, {
            const E: LspRawPureHandler<$s, JsonValue> = |this, input| {
                let input: <$desc as lsp_types::notification::Notification>::Params =
                    serde_json::from_value(input).unwrap(); // todo: soft unwrap
                this.$method(input)
            };
            E
        })
    };
}
use notify_fn;

// #[macro_export]
// macro_rules! request_fn {
//     ($desc: ty, Self::$method: ident) => {
//         (<$desc>::METHOD, {
//             const E: LspMethod<JsonValue> = |this, req| {
//                 let req: <$desc as lsp_types::request::Request>::Params =
//                     serde_json::from_value(req).unwrap(); // todo: soft
// unwrap                 this.$method(req)
//             };
//             E
//         })
//     };
// }

// #[macro_export]
// macro_rules! notify_fn {
//     ($desc: ty, Self::$method: ident) => {
//         (<$desc>::METHOD, {
//             const E: LspMethod<()> = |this, input| {
//                 let input: <$desc as
// lsp_types::notification::Notification>::Params =
// serde_json::from_value(input).unwrap(); // todo: soft unwrap
// this.$method(input)             };
//             E
//         })
//     };
// }

use crate::{just_ok, just_result};

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

use crate::{LspResponseFuture, LspResult};
