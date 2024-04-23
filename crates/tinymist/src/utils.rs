use tokio::sync::oneshot;
use typst_ts_core::error::prelude::*;
use typst_ts_core::Error;

pub fn threaded_receive<T: Send>(f: oneshot::Receiver<T>) -> Result<T, Error> {
    // get current async handle
    if let Ok(e) = tokio::runtime::Handle::try_current() {
        // todo: remove blocking
        let mut ret = None;
        rayon::scope(|s| {
            s.spawn(|_| {
                ret = Some(
                    e.block_on(f)
                        .map_err(map_string_err("failed to sync_render")),
                )
            })
        });
        return ret.unwrap();
    }

    f.blocking_recv()
        .map_err(map_string_err("failed to recv from sync_render"))
}
