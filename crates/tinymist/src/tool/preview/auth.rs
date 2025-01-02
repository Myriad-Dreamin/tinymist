use anyhow::{Context, Result};
use futures::SinkExt;
use futures::StreamExt;
use hyper_tungstenite::{tungstenite::Message, HyperWebsocketStream};
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

const TOKEN_LENGTH: usize = 50;

pub fn generate_token() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), TOKEN_LENGTH)
}

fn sha512hex(text: &str) -> String {
    let hash = Sha512::digest(text);
    format!("{:x}", hash)
}

#[derive(Serialize, Debug)]
struct AuthMsgChallenge<'a> {
    challenge: &'a str,
}

#[derive(Deserialize, Debug)]
struct AuthMsgResponseClient<'a> {
    hash: &'a str,
    challenge: &'a str,
    cnonce: &'a str,
}

#[derive(Serialize, Debug)]
struct AuthMsgResponseServer<'a> {
    hash: &'a str,
    snonce: &'a str,
}

#[derive(Serialize, Debug)]
struct AuthFailResponseServer {
    auth: bool,
}

#[derive(Debug, Clone)]
struct AuthError;

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Authentication failed")
    }
}

impl std::error::Error for AuthError {}

/// Websocket Authentication
///
/// This a) authenticates the client to us and b) we (the server) authenticate ourselves to the client.
///
/// a) is important so that we don't send any sensitive information to clients that are not supposed to know that information.
///    For example, this protects against the fact that browsers allow any website to connect to websockets on 127.0.0.1
/// b) is important because on multi-user systems another user can potentially impersonate us (the server) and trick the client
///    into sending private information to that other server instead of to us.
///
/// This function takes an owned `HyperWebsocketStream` to avoid accidental misuse elsewhere before authentication.
///
/// TODO: This is a basic challenge response authentication with nonces. Is there something more standard we could use?
pub async fn try_auth_websocket_client(
    mut websocket: HyperWebsocketStream,
    secret: &str,
) -> Result<HyperWebsocketStream> {
    // First we send the client a challenge to authenticate the client to us ...
    let challenge = generate_token();
    let json = serde_json::to_string(&AuthMsgChallenge {
        challenge: &challenge,
    })?;
    websocket.send(Message::Binary(json.into())).await?;

    let response = websocket
        .next()
        .await
        .context("auth response 1 missing")??;
    let response: AuthMsgResponseClient = serde_json::from_str(response.to_text()?)?;

    if sha512hex(format!("{}:{}:{}", secret, challenge, response.cnonce).as_str()) == response.hash
    {
        // ... then we authenticate to the client
        let snonce = generate_token();
        let hash = sha512hex(format!("{}:{}:{}", secret, response.challenge, snonce).as_str());
        let json = serde_json::to_string(&AuthMsgResponseServer {
            snonce: &snonce,
            hash: &hash,
        })?;
        websocket.send(Message::Binary(json.into())).await?;

        Ok(websocket)
    } else {
        log::info!("Websocket client authentication failed.");
        let json = serde_json::to_string(&AuthFailResponseServer { auth: false })?;
        websocket.send(Message::Binary(json.into())).await?;
        Err(AuthError.into())
    }
}
