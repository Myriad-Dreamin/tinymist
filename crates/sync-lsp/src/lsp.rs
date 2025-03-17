#![allow(missing_docs)]

use std::io::{self, BufRead, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{
    invalid_data_fmt, read_msg_text, write_msg_text, ExtractError, LspOrDapResponse, RequestId,
    ResponseError,
};

/// A message in the Language Server Protocol.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Message {
    /// Request messages
    Request(Request),
    /// Response messages
    Response(Response),
    /// Notification messages
    Notification(Notification),
}

impl From<Request> for Message {
    fn from(request: Request) -> Message {
        Message::Request(request)
    }
}

impl From<Response> for Message {
    fn from(response: Response) -> Message {
        Message::Response(response)
    }
}

impl From<Notification> for Message {
    fn from(notification: Notification) -> Message {
        Message::Notification(notification)
    }
}

/// A request in the Language Server Protocol.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Request {
    pub id: RequestId,
    pub method: String,
    #[serde(default = "serde_json::Value::default")]
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Response {
    // JSON RPC allows this to be null if it was impossible
    // to decode the request's id. Ignore this special case
    // and just die horribly.
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponseError>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Notification {
    pub method: String,
    #[serde(default = "serde_json::Value::default")]
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
}

impl Message {
    pub fn read(r: &mut impl BufRead) -> io::Result<Option<Message>> {
        Message::_read(r)
    }
    fn _read(r: &mut dyn BufRead) -> io::Result<Option<Message>> {
        let text = match read_msg_text(r)? {
            None => return Ok(None),
            Some(text) => text,
        };

        let msg = match serde_json::from_str(&text) {
            Ok(msg) => msg,
            Err(e) => {
                return Err(invalid_data_fmt!("malformed LSP payload: {e:?}"));
            }
        };

        Ok(Some(msg))
    }
    pub fn write(self, w: &mut impl Write) -> io::Result<()> {
        self._write(w)
    }
    fn _write(self, w: &mut dyn Write) -> io::Result<()> {
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

impl Response {
    pub fn new_ok<R: serde::Serialize>(id: RequestId, result: R) -> Response {
        Response {
            id,
            result: Some(serde_json::to_value(result).unwrap()),
            error: None,
        }
    }
    pub fn new_err(id: RequestId, code: i32, message: String) -> Response {
        let error = ResponseError {
            code,
            message,
            data: None,
        };
        Response {
            id,
            result: None,
            error: Some(error),
        }
    }
}

impl Request {
    pub fn new<P: serde::Serialize>(id: RequestId, method: String, params: P) -> Request {
        Request {
            id,
            method,
            params: serde_json::to_value(params).unwrap(),
        }
    }
    pub fn extract<P: DeserializeOwned>(
        self,
        method: &str,
    ) -> Result<(RequestId, P), ExtractError<Request>> {
        if self.method != method {
            return Err(ExtractError::MethodMismatch(self));
        }
        match serde_json::from_value(self.params) {
            Ok(params) => Ok((self.id, params)),
            Err(error) => Err(ExtractError::JsonError {
                method: self.method,
                error,
            }),
        }
    }
}

impl Notification {
    pub fn new(method: String, params: impl serde::Serialize) -> Notification {
        Notification {
            method,
            params: serde_json::to_value(params).unwrap(),
        }
    }
    pub fn extract<P: DeserializeOwned>(
        self,
        method: &str,
    ) -> Result<P, ExtractError<Notification>> {
        if self.method != method {
            return Err(ExtractError::MethodMismatch(self));
        }
        match serde_json::from_value(self.params) {
            Ok(params) => Ok(params),
            Err(error) => Err(ExtractError::JsonError {
                method: self.method,
                error,
            }),
        }
    }
    pub(crate) fn is_exit(&self) -> bool {
        self.method == "exit"
    }
}

impl From<Response> for LspOrDapResponse {
    fn from(resp: Response) -> Self {
        Self::Lsp(resp)
    }
}

impl TryFrom<LspOrDapResponse> for Response {
    type Error = anyhow::Error;

    fn try_from(resp: LspOrDapResponse) -> anyhow::Result<Self> {
        match resp {
            LspOrDapResponse::Lsp(resp) => Ok(resp),
            #[cfg(feature = "dap")]
            LspOrDapResponse::Dap(_) => anyhow::bail!("unexpected DAP response"),
        }
    }
}

impl From<Request> for crate::Message {
    fn from(request: Request) -> crate::Message {
        crate::Message::Lsp(request.into())
    }
}

impl From<Response> for crate::Message {
    fn from(response: Response) -> crate::Message {
        crate::Message::Lsp(response.into())
    }
}

impl From<Notification> for crate::Message {
    fn from(notification: Notification) -> crate::Message {
        crate::Message::Lsp(notification.into())
    }
}

impl TryFrom<crate::Message> for Message {
    type Error = anyhow::Error;

    fn try_from(msg: crate::Message) -> anyhow::Result<Self> {
        match msg {
            crate::Message::Lsp(msg) => Ok(msg),
            #[cfg(feature = "dap")]
            crate::Message::Dap(msg) => anyhow::bail!("unexpected DAP message: {msg:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Message, Notification, Request, RequestId};

    #[test]
    fn shutdown_with_explicit_null() {
        let text = "{\"jsonrpc\": \"2.0\",\"id\": 3,\"method\": \"shutdown\", \"params\": null }";
        let msg: Message = serde_json::from_str(text).unwrap();

        assert!(
            matches!(msg, Message::Request(req) if req.id == 3.into() && req.method == "shutdown")
        );
    }

    #[test]
    fn shutdown_with_no_params() {
        let text = "{\"jsonrpc\": \"2.0\",\"id\": 3,\"method\": \"shutdown\"}";
        let msg: Message = serde_json::from_str(text).unwrap();

        assert!(
            matches!(msg, Message::Request(req) if req.id == 3.into() && req.method == "shutdown")
        );
    }

    #[test]
    fn notification_with_explicit_null() {
        let text = "{\"jsonrpc\": \"2.0\",\"method\": \"exit\", \"params\": null }";
        let msg: Message = serde_json::from_str(text).unwrap();

        assert!(matches!(msg, Message::Notification(not) if not.method == "exit"));
    }

    #[test]
    fn notification_with_no_params() {
        let text = "{\"jsonrpc\": \"2.0\",\"method\": \"exit\"}";
        let msg: Message = serde_json::from_str(text).unwrap();

        assert!(matches!(msg, Message::Notification(not) if not.method == "exit"));
    }

    #[test]
    fn serialize_request_with_null_params() {
        let msg = Message::Request(Request {
            id: RequestId::from(3),
            method: "shutdown".into(),
            params: serde_json::Value::Null,
        });
        let serialized = serde_json::to_string(&msg).unwrap();

        assert_eq!("{\"id\":3,\"method\":\"shutdown\"}", serialized);
    }

    #[test]
    fn serialize_notification_with_null_params() {
        let msg = Message::Notification(Notification {
            method: "exit".into(),
            params: serde_json::Value::Null,
        });
        let serialized = serde_json::to_string(&msg).unwrap();

        assert_eq!("{\"method\":\"exit\"}", serialized);
    }
}
