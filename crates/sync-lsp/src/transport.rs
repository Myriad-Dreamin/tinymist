//! Transport layer for LSP messages.

use std::{
    io::{self, BufRead, Read, Write},
    thread,
};

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};

use crate::{Connection, ConnectionRx, ConnectionTx, GetMessageKind, Message};

/// Convenience cli arguments for setting up a transport with an optional mirror
/// or replay file.
///
/// The `mirror` argument will write the stdin to the file.
/// The `replay` argument will read the file as input.
///
/// # Example
///
/// The example below shows the typical usage of the `MirrorArgs` struct.
/// It records an LSP or DAP session and replays it to compare the output.
///
/// If the language server has stable output, the replayed output should be the
/// same.
///
/// ```bash
/// $ my-lsp --mirror /tmp/mirror.log > responses.txt
/// $ ls /tmp
/// mirror.log
/// $ my-lsp --replay /tmp/mirror.log > responses-replayed.txt
/// $ diff responses.txt responses-replayed.txt
/// ```
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

/// Note that we must have our logging only write out to stderr.
pub fn with_stdio_transport<M: TryFrom<Message, Error = anyhow::Error> + GetMessageKind>(
    args: MirrorArgs,
    f: impl FnOnce(Connection<M>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    with_stdio_transport_impl(args, M::get_message_kind(), |conn| f(conn.into()))
}

/// Note that we must have our logging only write out to stderr.
fn with_stdio_transport_impl(
    args: MirrorArgs,
    kind: crate::MessageKind,
    f: impl FnOnce(Connection<Message>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    // Set up input and output
    let replay = args.replay.clone();
    let mirror = args.mirror.clone();
    let i = move || -> Box<dyn BufRead> {
        if !replay.is_empty() {
            // Get input from file
            let file = std::fs::File::open(&replay).unwrap();
            let file = std::io::BufReader::new(file);
            Box::new(file)
        } else if mirror.is_empty() {
            // Get input from stdin
            let stdin = std::io::stdin().lock();
            Box::new(stdin)
        } else {
            let file = std::fs::File::create(&mirror).unwrap();
            let stdin = std::io::stdin().lock();
            Box::new(MirrorWriter(stdin, file, std::sync::Once::new()))
        }
    };
    let o = || std::io::stdout().lock();

    let (event_sender, event_receiver) = unbounded::<crate::Event>();

    // Create the transport. Includes the stdio (stdin and stdout) versions but this
    // could also be implemented to use sockets or HTTP.
    let (lsp_sender, lsp_receiver, io_threads) = io_transport(kind, i, o);
    let connection = Connection {
        // lsp_sender,
        // lsp_receiver,
        sender: ConnectionTx {
            event: event_sender,
            lsp: lsp_sender,
            marker: std::marker::PhantomData,
        },
        receiver: ConnectionRx {
            event: event_receiver,
            lsp: lsp_receiver,
            marker: std::marker::PhantomData,
        },
    };

    f(connection)?;

    io_threads.join_write()?;

    Ok(())
}

/// Creates an LSP connection via io.
///
/// # Example
///
/// ```
/// use std::io::{stdin, stdout};
/// use sync_ls::transport::{io_transport, IoThreads};
/// use lsp_server::Message;
/// use crossbeam_channel::{bounded, Receiver, Sender};
/// pub fn stdio_transport() -> (Sender<Message>, Receiver<Message>, IoThreads) {
///   io_transport(|| stdin().lock(), || stdout().lock())
/// }
/// ```
pub fn io_transport<I: BufRead, O: Write>(
    kind: crate::MessageKind,
    inp: impl FnOnce() -> I + Send + Sync + 'static,
    out: impl FnOnce() -> O + Send + Sync + 'static,
) -> (Sender<Message>, Receiver<Message>, IoThreads) {
    let (writer_sender, writer_receiver) = bounded::<Message>(0);
    let writer = thread::spawn(move || {
        let mut out = out();
        let res = writer_receiver
            .into_iter()
            .try_for_each(|it| it.write(&mut out));

        log::info!("writer thread finished");
        res
    });
    let (reader_sender, reader_receiver) = bounded::<Message>(0);
    let reader = thread::spawn(move || {
        let mut inp = inp();
        let read_impl = match kind {
            #[cfg(feature = "lsp")]
            crate::MessageKind::Lsp => Message::read_lsp::<I>,
            #[cfg(feature = "dap")]
            crate::MessageKind::Dap => Message::read_dap::<I>,
        };
        while let Some(msg) = read_impl(&mut inp)? {
            #[cfg(feature = "lsp")]
            use crate::LspMessage;
            #[cfg(feature = "lsp")]
            let is_exit = matches!(&msg, Message::Lsp(LspMessage::Notification(n)) if n.is_exit());

            log::trace!("sending message {:#?}", msg);
            reader_sender
                .send(msg)
                .expect("receiver was dropped, failed to send a message");

            #[cfg(feature = "lsp")]
            if is_exit {
                break;
            }
        }

        log::info!("reader thread finished");
        Ok(())
    });
    let threads = IoThreads { reader, writer };
    (writer_sender, reader_receiver, threads)
}

/// A pair of threads for reading and writing LSP messages.
pub struct IoThreads {
    reader: thread::JoinHandle<io::Result<()>>,
    writer: thread::JoinHandle<io::Result<()>>,
}

impl IoThreads {
    /// Waits for the reader and writer threads to finish.
    pub fn join(self) -> io::Result<()> {
        match self.reader.join() {
            Ok(r) => r?,
            Err(err) => {
                eprintln!("reader panicked!");
                std::panic::panic_any(err)
            }
        }
        match self.writer.join() {
            Ok(r) => r,
            Err(err) => {
                eprintln!("writer panicked!");
                std::panic::panic_any(err);
            }
        }
    }

    /// Waits for the writer threads to finish.
    pub fn join_write(self) -> io::Result<()> {
        match self.writer.join() {
            Ok(r) => r,
            Err(err) => {
                eprintln!("writer panicked!");
                std::panic::panic_any(err);
            }
        }
    }
}

struct MirrorWriter<R: Read, W: Write>(R, W, std::sync::Once);

impl<R: Read, W: Write> Read for MirrorWriter<R, W> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let res = self.0.read(buf)?;

        if let Err(err) = self.1.write_all(&buf[..res]) {
            self.2.call_once(|| {
                log::warn!("failed to write to mirror: {err}");
            });
        }

        Ok(res)
    }
}

impl<R: Read + BufRead, W: Write> BufRead for MirrorWriter<R, W> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.0.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        let buf = self.0.fill_buf().unwrap();

        if let Err(err) = self.1.write_all(&buf[..amt]) {
            self.2.call_once(|| {
                log::warn!("failed to write to mirror: {err}");
            });
        }

        self.0.consume(amt);
    }
}
