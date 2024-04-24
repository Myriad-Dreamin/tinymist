use std::thread;

use tokio::sync::oneshot;
use typst_ts_core::error::prelude::*;
use typst_ts_core::Error;

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
