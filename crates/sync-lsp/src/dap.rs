//! A synchronous debug adaptor server implementation.

use std::io;

use serde::{Deserialize, Serialize};

pub use dapts::{Event, Request, Response};

use crate::{invalid_data_fmt, read_msg_text, write_msg_text, LspOrDapResponse};

/// A message in the Debug Adaptor Protocol.
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
    /// Reads a DAP message from the reader.
    pub fn read(r: &mut impl io::BufRead) -> io::Result<Option<Message>> {
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
    /// Writes the DAP message to the writer.
    pub fn write(self, w: &mut impl io::Write) -> io::Result<()> {
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

impl TryFrom<crate::Message> for Message {
    type Error = anyhow::Error;

    fn try_from(msg: crate::Message) -> anyhow::Result<Self> {
        match msg {
            #[cfg(feature = "lsp")]
            crate::Message::Lsp(msg) => anyhow::bail!("unexpected LSP message: {msg:?}"),
            crate::Message::Dap(msg) => Ok(msg),
        }
    }
}

impl From<Request> for crate::Message {
    fn from(request: Request) -> crate::Message {
        crate::Message::Dap(request.into())
    }
}

impl From<Response> for crate::Message {
    fn from(response: Response) -> crate::Message {
        crate::Message::Dap(response.into())
    }
}

impl From<Event> for crate::Message {
    fn from(notification: Event) -> crate::Message {
        crate::Message::Dap(notification.into())
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
