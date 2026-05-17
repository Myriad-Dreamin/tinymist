use core::fmt;

#[cfg(all(feature = "system", feature = "trace"))]
use std::pin::Pin;
#[cfg(all(feature = "system", feature = "trace"))]
use std::sync::atomic::AtomicU64;
#[cfg(all(feature = "system", feature = "trace"))]
use std::sync::Arc;
#[cfg(all(feature = "system", feature = "trace"))]
use std::task::{Context, Poll};
#[cfg(all(feature = "system", feature = "trace"))]
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
#[cfg(all(feature = "system", feature = "trace"))]
use tokio::net::TcpStream;
#[cfg(all(feature = "system", feature = "trace"))]
use tokio_util::sync::CancellationToken;

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

#[cfg(all(feature = "system", feature = "trace"))]
#[derive(Default)]
pub(crate) struct AliveLock(Arc<AtomicU64>);

#[cfg(all(feature = "system", feature = "trace"))]
impl AliveLock {
    pub fn hold(cnt: Arc<AtomicU64>) -> Self {
        let held = cnt.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        log::info!("alive lock held, count: {held}");
        Self(cnt.clone())
    }
}

#[cfg(all(feature = "system", feature = "trace"))]
impl Drop for AliveLock {
    fn drop(&mut self) {
        let cnt = self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        log::info!("alive lock dropped, count: {cnt}");
    }
}

#[cfg(all(feature = "system", feature = "trace"))]
pub(crate) struct ConnWithCancel {
    stream: TcpStream,
    pub cancel: CancellationToken,
}

#[cfg(all(feature = "system", feature = "trace"))]
impl ConnWithCancel {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            cancel: CancellationToken::new(),
        }
    }
}

#[cfg(all(feature = "system", feature = "trace"))]
impl Drop for ConnWithCancel {
    fn drop(&mut self) {
        self.cancel.cancel()
    }
}

#[cfg(all(feature = "system", feature = "trace"))]
impl AsyncRead for ConnWithCancel {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<tokio::io::Result<()>> {
        Pin::new(&mut Pin::into_inner(self).stream).poll_read(context, buf)
    }
}

#[cfg(all(feature = "system", feature = "trace"))]
impl AsyncWrite for ConnWithCancel {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, tokio::io::Error>> {
        Pin::new(&mut Pin::into_inner(self).stream).poll_write(context, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), tokio::io::Error>> {
        Pin::new(&mut Pin::into_inner(self).stream).poll_flush(context)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), tokio::io::Error>> {
        Pin::new(&mut Pin::into_inner(self).stream).poll_shutdown(context)
    }
}
