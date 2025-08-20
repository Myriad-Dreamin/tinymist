use core::fmt;
use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use sync_ls::{
    GetMessageKind, LsHook, LspClientRoot, LspResult, Message, RequestId, TConnectionTx,
};
use tinymist_std::hash::{FxBuildHasher, FxHashMap};
use typst::ecow::EcoString;

use crate::*;

/// Creates a new client root (connection).
pub fn client_root<M: TryFrom<Message, Error = anyhow::Error> + GetMessageKind>(
    sender: TConnectionTx<M>,
) -> LspClientRoot {
    LspClientRoot::new(RUNTIMES.tokio_runtime.handle().clone(), sender)
        .with_hook(Arc::new(TypstLsHook::default()))
}

#[derive(Default)]
struct TypstLsHook(Mutex<FxHashMap<RequestId, typst_timing::TimingScope>>);

impl fmt::Debug for TypstLsHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TypstLsHook").finish()
    }
}

impl LsHook for TypstLsHook {
    fn start_request(&self, req_id: &RequestId, method: &str) {
        ().start_request(req_id, method);

        if let Some(scope) = typst_timing::TimingScope::new(static_str(method)) {
            let mut map = self.0.lock();
            map.insert(req_id.clone(), scope);
        }
    }

    fn stop_request(
        &self,
        req_id: &RequestId,
        method: &str,
        received_at: tinymist_std::time::Instant,
    ) {
        ().stop_request(req_id, method, received_at);

        if let Some(scope) = self.0.lock().remove(req_id) {
            let _ = scope;
        }
    }

    fn start_notification(&self, method: &str) {
        ().start_notification(method);
    }

    fn stop_notification(
        &self,
        method: &str,
        received_at: tinymist_std::time::Instant,
        result: LspResult<()>,
    ) {
        ().stop_notification(method, received_at, result);
    }
}

fn static_str(s: &str) -> &'static str {
    static STRS: Mutex<FxHashMap<EcoString, &'static str>> =
        Mutex::new(HashMap::with_hasher(FxBuildHasher));

    let mut strs = STRS.lock();
    if let Some(&s) = strs.get(s) {
        return s;
    }

    let static_ref: &'static str = String::from(s).leak();
    strs.insert(static_ref.into(), static_ref);
    static_ref
}
