use std::{
    io::{self, BufRead, Read, Write},
    thread,
};

use crossbeam_channel::{bounded, Receiver, Sender};
use lsp_server::{Connection, Message};

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
pub fn with_stdio_transport(
    args: MirrorArgs,
    f: impl FnOnce(Connection) -> anyhow::Result<()>,
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

    // Create the transport. Includes the stdio (stdin and stdout) versions but this
    // could also be implemented to use sockets or HTTP.
    let (sender, receiver, io_threads) = io_transport(i, o);
    let connection = Connection { sender, receiver };

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
/// use tinymist::transport::{io_transport, IoThreads};
/// use lsp_server::Message;
/// use crossbeam_channel::{bounded, Receiver, Sender};
/// pub fn stdio_transport() -> (Sender<Message>, Receiver<Message>, IoThreads) {
///   io_transport(|| stdin().lock(), || stdout().lock())
/// }
/// ```
pub fn io_transport<I: BufRead, O: Write>(
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
        while let Some(msg) = Message::read(&mut inp)? {
            let is_exit = matches!(&msg, Message::Notification(n) if n.method == "exit");

            log::trace!("sending message {:#?}", msg);
            reader_sender
                .send(msg)
                .expect("receiver was dropped, failed to send a message");

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
