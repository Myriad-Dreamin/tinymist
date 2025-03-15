//! Message from and to language servers and clients.

use std::io::{self, BufRead, Write};

use crate::{dap, lsp};

/// The kind of the message.
pub enum MessageKind {
    /// A message in the LSP protocol.
    Lsp,
    /// A message in the DAP protocol.
    Dap,
}

/// Gets the kind of the message.
pub trait GetMessageKind {
    /// Returns the kind of the message.
    fn get_message_kind() -> MessageKind;
}

impl GetMessageKind for LspMessage {
    fn get_message_kind() -> MessageKind {
        MessageKind::Lsp
    }
}

impl GetMessageKind for dap::Message {
    fn get_message_kind() -> MessageKind {
        MessageKind::Dap
    }
}

/// The common message type for the LSP protocol.
pub type LspMessage = lsp::Message;

/// The common message type for the DAP protocol.
pub type DapMessage = dap::Message;

/// The common message type for the language server.
#[derive(Debug)]
pub enum Message {
    /// A message in the LSP protocol.
    Lsp(LspMessage),
    /// A message in the DAP protocol.
    #[cfg(feature = "dap")]
    Dap(dap::Message),
}

impl From<lsp::Request> for Message {
    fn from(request: lsp::Request) -> Message {
        Message::Lsp(request.into())
    }
}

impl From<lsp::Response> for Message {
    fn from(response: lsp::Response) -> Message {
        Message::Lsp(response.into())
    }
}

impl From<lsp::Notification> for Message {
    fn from(notification: lsp::Notification) -> Message {
        Message::Lsp(notification.into())
    }
}

#[cfg(feature = "lsp")]
impl TryFrom<Message> for LspMessage {
    type Error = anyhow::Error;

    fn try_from(msg: Message) -> anyhow::Result<Self> {
        match msg {
            Message::Lsp(msg) => Ok(msg),
            #[cfg(feature = "dap")]
            Message::Dap(msg) => anyhow::bail!("unexpected DAP message: {msg:?}"),
        }
    }
}

#[cfg(feature = "dap")]
impl TryFrom<Message> for DapMessage {
    type Error = anyhow::Error;

    fn try_from(msg: Message) -> anyhow::Result<Self> {
        match msg {
            #[cfg(feature = "lsp")]
            Message::Lsp(msg) => anyhow::bail!("unexpected LSP message: {msg:?}"),
            Message::Dap(msg) => Ok(msg),
        }
    }
}

impl Message {
    /// Reads a lsp message from the given reader.
    pub fn read_lsp<R: std::io::BufRead>(reader: &mut R) -> std::io::Result<Option<Self>> {
        let msg = lsp::Message::read(reader)?;
        Ok(msg.map(Message::Lsp))
    }

    /// Reads a dap message from the given reader.
    pub fn read_dap<R: std::io::BufRead>(reader: &mut R) -> std::io::Result<Option<Self>> {
        let msg = dap::Message::read(reader)?;
        Ok(msg.map(Message::Dap))
    }

    /// Writes the message to the given writer.
    pub fn write<W: std::io::Write>(self, writer: &mut W) -> std::io::Result<()> {
        match self {
            Message::Lsp(msg) => msg.write(writer),
            #[cfg(feature = "dap")]
            Message::Dap(msg) => msg.write(writer),
        }
    }
}

pub(crate) enum LspOrDapResponse {
    #[cfg(feature = "lsp")]
    Lsp(lsp::Response),
    #[cfg(feature = "dap")]
    Dap(dap::Response),
}

pub(crate) fn read_msg_text(inp: &mut dyn BufRead) -> io::Result<Option<String>> {
    let mut size = None;
    let mut buf = String::new();
    loop {
        buf.clear();
        if inp.read_line(&mut buf)? == 0 {
            return Ok(None);
        }
        if !buf.ends_with("\r\n") {
            return Err(invalid_data_fmt!("malformed header: {buf:?}"));
        }
        let buf = &buf[..buf.len() - 2];
        if buf.is_empty() {
            break;
        }
        let mut parts = buf.splitn(2, ": ");
        let header_name = parts.next().unwrap();
        let header_value = parts
            .next()
            .ok_or_else(|| invalid_data_fmt!("malformed header: {buf:?}"))?;
        if header_name.eq_ignore_ascii_case("Content-Length") {
            size = Some(header_value.parse::<usize>().map_err(invalid_data)?);
        }
    }
    let size: usize = size.ok_or_else(|| invalid_data_fmt!("no Content-Length"))?;
    let mut buf = buf.into_bytes();
    buf.resize(size, 0);
    inp.read_exact(&mut buf)?;
    let buf = String::from_utf8(buf).map_err(invalid_data)?;
    log::debug!("< {}", buf);
    Ok(Some(buf))
}

pub(crate) fn write_msg_text(out: &mut dyn Write, msg: &str) -> io::Result<()> {
    log::debug!("> {}", msg);
    write!(out, "Content-Length: {}\r\n\r\n", msg.len())?;
    out.write_all(msg.as_bytes())?;
    out.flush()?;
    Ok(())
}

pub(crate) fn invalid_data(
    error: impl Into<Box<dyn std::error::Error + Send + Sync>>,
) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

macro_rules! invalid_data_fmt {
    ($($tt:tt)*) => ($crate::invalid_data(format!($($tt)*)))
}
pub(crate) use invalid_data_fmt;
