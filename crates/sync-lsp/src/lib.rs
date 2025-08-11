//! A synchronous language server implementation.

#[cfg(feature = "dap")]
pub mod dap;
#[cfg(feature = "lsp")]
pub mod lsp;

mod error;
pub use error::*;

mod msg;
pub use msg::*;

#[cfg(feature = "server")]
pub use server::*;
#[cfg(feature = "server")]
pub mod req_queue;
#[cfg(feature = "server")]
mod server;
#[cfg(all(feature = "server", feature = "system"))]
pub mod transport;

use std::any::Any;

/// The common error type for language servers.
pub use crate::msg::ResponseError;
/// The common result type for language servers.
pub type LspResult<T> = Result<T, ResponseError>;
/// The common event type for language servers.
pub type Event = Box<dyn Any + Send>;

/// Note that we must have our logging only write out to stderr.
#[cfg(feature = "web")]
fn dummy_transport<M: TryFrom<Message, Error = anyhow::Error> + GetMessageKind>() -> Connection<M> {
    let (event_sender, event_receiver) = crossbeam_channel::bounded::<crate::Event>(0);
    let (writer_sender, writer_receiver) = crossbeam_channel::bounded::<Message>(0);
    Connection {
        // lsp_sender,
        // lsp_receiver,
        sender: TConnectionTx {
            event: event_sender,
            lsp: writer_sender,
            marker: std::marker::PhantomData,
        },
        receiver: TConnectionRx {
            event: event_receiver,
            lsp: writer_receiver,
            marker: std::marker::PhantomData,
        },
    }
}
