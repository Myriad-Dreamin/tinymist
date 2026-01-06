use core::fmt;
use std::collections::{HashMap, VecDeque};
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
        .with_hook(Arc::new(TinymistHook::default()))
}

#[derive(Default)]
struct TinymistHook {
    /// Data for stalling tracking.
    may_stall: Mutex<VecDeque<(RequestId, tinymist_std::time::Time)>>,
    /// Whether finished for stalling tracking.
    stall_data: Mutex<FxHashMap<RequestId, StallTab>>,
    /// Data for performance tracking.
    perf: Mutex<FxHashMap<RequestId, typst_timing::TimingScope>>,
}

impl fmt::Debug for TinymistHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TinymistHook").finish()
    }
}

impl LsHook for TinymistHook {
    fn start_request(&self, req_id: &RequestId, method: &str) {
        self.start_stall(req_id.clone(), method);

        if let Some(scope) = typst_timing::TimingScope::new(static_str(method)) {
            let mut map = self.perf.lock();
            map.insert(req_id.clone(), scope);
        }
    }

    fn stop_request(
        &self,
        req_id: &RequestId,
        _method: &str,
        _received_at: tinymist_std::time::Instant,
    ) {
        self.stop_stall(req_id.clone());

        if let Some(scope) = self.perf.lock().remove(req_id) {
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

impl TinymistHook {
    fn start_stall(&self, msg_id: RequestId, method: &str) {
        let mut may_stall = self.may_stall.lock();
        let time = tinymist_std::time::now();
        may_stall.push_back((msg_id.clone(), time));
        self.stall_data.lock().insert(
            msg_id,
            StallTab {
                method: static_str(method),
                stalled: false,
            },
        );

        while !may_stall.is_empty() {
            // consume one anyway.
            let Some((id, since)) = may_stall.pop_front() else {
                break;
            };

            let elapsed = match time.duration_since(since) {
                Ok(elapsed) => elapsed,
                Err(err) => {
                    log::error!("failed to get elapsed time for stall tracking: {err}");
                    break;
                }
            };

            if elapsed.as_secs() > 10 {
                let mut stall_data = self.stall_data.lock();
                let Some(tab) = stall_data.get_mut(&id) else {
                    continue;
                };
                log::warn!(
                    "stall detected: {id:?}, method: {:?}, since: {since:?}, elapsed: {elapsed:?}",
                    tab.method
                );
                tab.stalled = true;
            } else {
                // This is free, because vecqueue is a ring buffer.
                may_stall.push_back((id, since));
                break;
            }
        }
    }

    fn stop_stall(&self, msg_id: RequestId) {
        let mut _may_stall = self.may_stall.lock();
        let result = self.stall_data.lock().remove(&msg_id);
        if let Some(tab) = result
            && tab.stalled
        {
            log::info!(
                "stalling request {msg_id:?} finished, method: {:?}",
                tab.method
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct StallTab {
    /// The method of the request to detect the stall.
    method: &'static str,
    /// Whether the request is stalled.
    stalled: bool,
}

fn static_str(s: &str) -> &'static str {
    static STRS: Mutex<FxHashMap<EcoString, &'static str>> =
        Mutex::new(HashMap::with_hasher(FxBuildHasher));

    let mut strs = STRS.lock();
    if let Some(&s) = strs.get(s) {
        return s;
    }

    let static_ref: &'static str = String::from(s).leak();
    strs.insert(s.into(), static_ref);
    static_ref
}
