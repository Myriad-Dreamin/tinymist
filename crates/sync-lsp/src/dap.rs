//! A synchronous debug adaptor implementation.

use std::io;

use serde::{Deserialize, Serialize};

// pub use dapts::{Event, Request, Response};

use crate::{invalid_data_fmt, read_msg_text, write_msg_text, LspOrDapResponse};

/// Represents a request from the client.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Request {
    /// Sequence number for the Request.
    ///
    /// From the [specification](https://microsoft.github.io/debug-adapter-protocol/specification#Base_Protocol_ProtocolMessage):
    ///
    /// Sequence number of the message (also known as message ID). The `seq` for
    /// the first message sent by a client or debug adapter is 1, and for each
    /// subsequent message is 1 greater than the previous message sent by that
    /// actor. `seq` can be used to order requests, responses, and events, and
    /// to associate requests with their corresponding responses. For
    /// protocol messages of type `request` the sequence number can be used
    /// to cancel the request.
    pub seq: i64,
    /// The command to execute.
    pub command: String,
    /// The command to execute.
    #[serde(default = "serde_json::Value::default")]
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub arguments: serde_json::Value,
}

/// Represents response to the client.
///
/// The command field (which is a string) is used as a tag in the ResponseBody
/// enum, so users of this crate will control it by selecting the appropriate
/// enum variant for the body.
///
/// There is also no separate `ErrorResponse` struct. Instead, `Error` is just a
/// variant of the ResponseBody enum.
///
/// Specification: [Response](https://microsoft.github.io/debug-adapter-protocol/specification#Base_Protocol_Response)
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    /// Sequence number of the corresponding request.
    #[serde(rename = "request_seq")]
    pub request_seq: i64,
    /// Outcome of the request.
    /// If true, the request was successful and the `body` attribute may contain
    /// the result of the request.
    /// If the value is false, the attribute `message` contains the error in
    /// short form and the `body` may contain additional information (see
    /// `ErrorResponse.body.error`).
    pub success: bool,
    /// Contains the raw error in short form if `success` is false.
    /// This raw error might be interpreted by the client and is not shown in
    /// the UI.
    /// Some predefined values exist.
    /// Values:
    /// 'cancelled': request was cancelled.
    /// etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<serde_json::Value>,
    /// Contains request result if success is true and error details if success
    /// is false.
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

/// Represents an event from the client.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Event {
    /// Sequence number for the Request.
    ///
    /// From the [specification](https://microsoft.github.io/debug-adapter-protocol/specification#Base_Protocol_ProtocolMessage):
    ///
    /// Sequence number of the message (also known as message ID). The `seq` for
    /// the first message sent by a client or debug adapter is 1, and for each
    /// subsequent message is 1 greater than the previous message sent by that
    /// actor. `seq` can be used to order requests, responses, and events, and
    /// to associate requests with their corresponding responses. For
    /// protocol messages of type `request` the sequence number can be used
    /// to cancel the request.
    pub seq: i64,
    /// Type of event.
    pub event: String,
    /// Event-specific information.
    #[serde(default = "serde_json::Value::default")]
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub body: serde_json::Value,
}

impl Event {
    /// Creates a new event.
    pub fn new(seq: i64, event: String, body: impl serde::Serialize) -> Event {
        Event {
            seq,
            event,
            body: serde_json::to_value(body).unwrap(),
        }
    }
}

/// Represents a DAP message.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Message {
    /// Request messages
    #[serde(rename = "request")]
    Request(Request),
    /// Response messages
    #[serde(rename = "response")]
    Response(Response),
    /// Event messages
    #[serde(rename = "event")]
    Event(Event),
}

impl From<Request> for Message {
    fn from(req: Request) -> Self {
        Message::Request(req)
    }
}

impl From<Response> for Message {
    fn from(resp: Response) -> Self {
        Message::Response(resp)
    }
}

impl From<Event> for Message {
    fn from(event: Event) -> Self {
        Message::Event(event)
    }
}

impl Message {
    /// Reads a dap message from the reader.
    pub fn read(r: &mut impl io::BufRead) -> io::Result<Option<Message>> {
        Message::_read(r)
    }
    fn _read(r: &mut dyn io::BufRead) -> io::Result<Option<Message>> {
        let text = match read_msg_text(r)? {
            None => return Ok(None),
            Some(text) => text,
        };

        let msg = match serde_json::from_str(&text) {
            Ok(msg) => msg,
            Err(e) => {
                return Err(invalid_data_fmt!("malformed DAP payload: {e:?}"));
            }
        };

        Ok(Some(msg))
    }
    /// Writes the message to the writer.
    pub fn write(self, w: &mut impl io::Write) -> io::Result<()> {
        self._write(w)
    }
    fn _write(self, w: &mut dyn io::Write) -> io::Result<()> {
        #[derive(Serialize)]
        struct JsonRpc {
            jsonrpc: &'static str,
            #[serde(flatten)]
            msg: Message,
        }
        let text = serde_json::to_string(&JsonRpc {
            jsonrpc: "2.0",
            msg: self,
        })?;
        write_msg_text(w, &text)
    }
}

impl From<Response> for LspOrDapResponse {
    fn from(resp: Response) -> Self {
        Self::Dap(resp)
    }
}

impl TryFrom<LspOrDapResponse> for Response {
    type Error = anyhow::Error;

    fn try_from(resp: LspOrDapResponse) -> anyhow::Result<Self> {
        match resp {
            #[cfg(feature = "lsp")]
            LspOrDapResponse::Lsp(_) => anyhow::bail!("unexpected LSP response"),
            LspOrDapResponse::Dap(resp) => Ok(resp),
        }
    }
}
