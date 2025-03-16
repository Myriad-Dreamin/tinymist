//! Message from and to language servers and clients.

use std::{
    fmt,
    io::{self, BufRead, Write},
};

use serde::{Deserialize, Serialize};

#[cfg(feature = "dap")]
use crate::dap;
#[cfg(feature = "lsp")]
use crate::lsp;

/// A request ID in the Language Server Protocol.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct RequestId(IdRepr);

impl RequestId {
    /// Converts the request ID back to the original dap type.
    #[cfg(feature = "dap")]
    pub fn dap(id: RequestId) -> i64 {
        match id.0 {
            IdRepr::I32(it) => it as i64,
            IdRepr::String(it) => panic!("unexpected string ID in DAP: {it}"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(untagged)]
enum IdRepr {
    I32(i32),
    String(String),
}

impl From<i32> for RequestId {
    fn from(id: i32) -> RequestId {
        RequestId(IdRepr::I32(id))
    }
}

impl From<String> for RequestId {
    fn from(id: String) -> RequestId {
        RequestId(IdRepr::String(id))
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            IdRepr::I32(it) => fmt::Display::fmt(it, f),
            // Use debug here, to make it clear that `92` and `"92"` are
            // different, and to reduce WTF factor if the sever uses `" "` as an
            // ID.
            IdRepr::String(it) => fmt::Debug::fmt(it, f),
        }
    }
}

/// A response from the server.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResponseError {
    /// The error code.
    pub code: i32,
    /// The error message.
    pub message: String,
    /// Additional data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// The error codes defined by the JSON RPC.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum ErrorCode {
    // Defined by JSON RPC:
    /// Invalid JSON was received by the server.
    ParseError = -32700,
    /// The JSON sent is not a valid Request object.
    InvalidRequest = -32600,
    /// The method does not exist / is not available.
    MethodNotFound = -32601,
    /// Invalid method parameter(s).
    InvalidParams = -32602,
    /// Internal JSON-RPC error.
    InternalError = -32603,
    /// The JSON sent is not a valid Request object.
    ServerErrorStart = -32099,
    /// The JSON sent is not a valid Request object.
    ServerErrorEnd = -32000,

    /// Error code indicating that a server received a notification or
    /// request before the server has received the `initialize` request.
    ServerNotInitialized = -32002,
    /// Error code indicating that a server received a request that
    /// is missing a required property.
    UnknownErrorCode = -32001,

    // Defined by the protocol:
    /// The client has canceled a request and a server has detected
    /// the cancel.
    RequestCanceled = -32800,

    /// The server detected that the content of a document got
    /// modified outside normal conditions. A server should
    /// NOT send this error code if it detects a content change
    /// in it unprocessed messages. The result even computed
    /// on an older state might still be useful for the client.
    ///
    /// If a client decides that a result is not of any use anymore
    /// the client should cancel the request.
    ContentModified = -32801,

    /// The server cancelled the request. This error code should
    /// only be used for requests that explicitly support being
    /// server cancellable.
    ///
    /// @since 3.17.0
    ServerCancelled = -32802,

    /// A request failed but it was syntactically correct, e.g the
    /// method name was known and the parameters were valid. The error
    /// message should contain human readable information about why
    /// the request failed.
    ///
    /// @since 3.17.0
    RequestFailed = -32803,
}

/// The kind of the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    /// A message in the LSP protocol.
    #[cfg(feature = "lsp")]
    Lsp,
    /// A message in the DAP protocol.
    #[cfg(feature = "dap")]
    Dap,
}

/// Gets the kind of the message.
pub trait GetMessageKind {
    /// Returns the kind of the message.
    fn get_message_kind() -> MessageKind;
}

#[cfg(feature = "lsp")]
impl GetMessageKind for LspMessage {
    fn get_message_kind() -> MessageKind {
        MessageKind::Lsp
    }
}

/// The common message type for the LSP protocol.
#[cfg(feature = "lsp")]
pub type LspMessage = lsp::Message;
/// The common message type for the DAP protocol.
#[cfg(feature = "dap")]
pub type DapMessage = dap::Message;

/// The common message type for the language server.
#[derive(Debug)]
pub enum Message {
    /// A message in the LSP protocol.
    #[cfg(feature = "lsp")]
    Lsp(LspMessage),
    /// A message in the DAP protocol.
    #[cfg(feature = "dap")]
    Dap(DapMessage),
}

impl Message {
    /// Reads a lsp message from the given reader.
    #[cfg(feature = "lsp")]
    pub fn read_lsp<R: std::io::BufRead>(reader: &mut R) -> std::io::Result<Option<Self>> {
        let msg = lsp::Message::read(reader)?;
        Ok(msg.map(Message::Lsp))
    }

    /// Reads a dap message from the given reader.
    #[cfg(feature = "dap")]
    pub fn read_dap<R: std::io::BufRead>(reader: &mut R) -> std::io::Result<Option<Self>> {
        let msg = dap::Message::read(reader)?;
        Ok(msg.map(Message::Dap))
    }

    /// Writes the message to the given writer.
    pub fn write<W: std::io::Write>(self, writer: &mut W) -> std::io::Result<()> {
        match self {
            #[cfg(feature = "lsp")]
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
