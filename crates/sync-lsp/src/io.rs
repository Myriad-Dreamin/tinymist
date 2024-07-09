use std::io::{self, Write};
use std::pin::Pin;
use std::task;

use futures::{AsyncRead, AsyncWrite};
use tokio_util::compat::TokioAsyncReadCompatExt;

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "clap", derive(clap::Parser))]
pub struct MirrorArgs {
    /// Mirror the stdin to the file
    #[cfg_attr(feature = "clap", clap(long, default_value = "", value_name = "FILE"))]
    pub mirror: String,
    /// Replay input from the file
    #[cfg_attr(feature = "clap", clap(long, default_value = "", value_name = "FILE"))]
    pub replay: String,
}

pub async fn get_io(args: MirrorArgs) -> (Pin<Box<dyn AsyncRead>>, Pin<Box<dyn AsyncWrite>>) {
    let input: Pin<Box<dyn AsyncRead>> = if !args.replay.is_empty() {
        // Get input from file.
        let file = tokio::fs::File::open(&args.replay).await.unwrap();
        Box::pin(TokioAsyncReadCompatExt::compat(file))
    } else {
        // Get input from stdin.
        #[cfg(unix)]
        let stdin = async_lsp::stdio::PipeStdin::lock_tokio().unwrap();
        #[cfg(not(unix))]
        let stdin = TokioAsyncReadCompatExt::compat(tokio::io::stdin());
        if !args.mirror.is_empty() {
            // Mirror to file.
            let file = std::fs::File::create(&args.replay).unwrap();
            Box::pin(MirrorWriter(Box::pin(stdin), file))
        } else {
            Box::pin(stdin)
        }
    };

    #[cfg(unix)]
    let stdout = async_lsp::stdio::PipeStdout::lock_tokio().unwrap();
    #[cfg(not(unix))]
    let stdout = tokio_util::compat::TokioAsyncWriteCompatExt::compat_write(tokio::io::stdout());

    (input, Box::pin(stdout))
}

// Pin<Box<R>> introduces an extra layer of indirection.
// But this is not a hotspot and it makes the code simpler.
struct MirrorWriter<R, W>(Pin<Box<R>>, W);

impl<R: AsyncRead, W: Write + Unpin> AsyncRead for MirrorWriter<R, W> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &mut [u8],
    ) -> task::Poll<io::Result<usize>> {
        let this = self.get_mut();

        // Read from input.
        let task::Poll::Ready(res) = this.0.as_mut().poll_read(cx, buf)? else {
            return task::Poll::Pending;
        };

        // Write to file.
        if let Err(err) = this.1.write(&buf[..res]) {
            log::warn!("failed to write to mirror: {err}");
        }

        task::Poll::Ready(Ok(res))
    }
}
