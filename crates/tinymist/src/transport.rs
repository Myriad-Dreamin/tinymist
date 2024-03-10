use std::{
    io::{self, BufRead, Write},
    thread,
};

use log::trace;

use crossbeam_channel::{bounded, Receiver, Sender};

use crate::Message;

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
    // todo: set cap back to 0
    let (writer_sender, writer_receiver) = bounded::<Message>(1024);
    let writer = thread::spawn(move || {
        let mut out = out();
        writer_receiver
            .into_iter()
            .try_for_each(|it| it.write(&mut out))
    });
    let (reader_sender, reader_receiver) = bounded::<Message>(1024);
    let reader = thread::spawn(move || {
        let mut inp = inp();
        while let Some(msg) = Message::read(&mut inp)? {
            let is_exit = matches!(&msg, Message::Notification(n) if n.method == "exit");

            trace!("sending message {:#?}", msg);
            reader_sender
                .send(msg)
                .expect("receiver was dropped, failed to send a message");

            if is_exit {
                break;
            }
        }
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
                println!("reader panicked!");
                std::panic::panic_any(err)
            }
        }
        match self.writer.join() {
            Ok(r) => r,
            Err(err) => {
                println!("writer panicked!");
                std::panic::panic_any(err);
            }
        }
    }
}
