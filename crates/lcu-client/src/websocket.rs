//! LCU WebSocket event feed.
//!
//! The League client pushes state changes over a WAMP-style WebSocket on the
//! same port as its REST API. Listening to it lets the host react the moment
//! the game flow, champ select or ready check changes instead of waiting for
//! the next polling tick.

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::Connector;

use crate::LockfileCredentials;

/// WAMP opcodes used by the LCU.
const WAMP_SUBSCRIBE: u64 = 5;
const WAMP_EVENT: u64 = 8;
const ALL_JSON_EVENTS: &str = "OnJsonApiEvent";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LcuEventKind {
    GameflowPhase,
    ChampSelect,
    ReadyCheck,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LcuEvent {
    pub kind: LcuEventKind,
    pub uri: String,
    /// `Create`, `Update` or `Delete`.
    pub event_type: String,
    pub data: Value,
}

fn classify_uri(uri: &str) -> Option<LcuEventKind> {
    if uri == "/lol-gameflow/v1/gameflow-phase" {
        Some(LcuEventKind::GameflowPhase)
    } else if uri == "/lol-champ-select/v1/session" {
        Some(LcuEventKind::ChampSelect)
    } else if uri == "/lol-matchmaking/v1/ready-check" {
        Some(LcuEventKind::ReadyCheck)
    } else {
        None
    }
}

/// Parse one text frame. Returns `None` for frames the host does not act on
/// (other URIs, non-event opcodes, malformed payloads).
pub fn parse_event_frame(text: &str) -> Option<LcuEvent> {
    let frame: Value = serde_json::from_str(text).ok()?;
    let parts = frame.as_array()?;
    if parts.first()?.as_u64()? != WAMP_EVENT {
        return None;
    }
    let payload = parts.get(2)?;
    let uri = payload.get("uri")?.as_str()?;
    let kind = classify_uri(uri)?;
    Some(LcuEvent {
        kind,
        uri: uri.to_string(),
        event_type: payload
            .get("eventType")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        data: payload.get("data").cloned().unwrap_or(Value::Null),
    })
}

/// Connect to the running client and call `on_event` for every relevant
/// event. Returns when the socket closes (client exit) or on error.
pub async fn listen(
    creds: &LockfileCredentials,
    on_event: impl FnMut(LcuEvent),
) -> Result<()> {
    let url = format!("wss://127.0.0.1:{}/", creds.port);
    listen_at(&url, Some(&creds.auth_header()), on_event).await
}

/// Same as [`listen`] for an explicit URL; `ws://` URLs skip TLS.
pub async fn listen_at(
    url: &str,
    auth_header: Option<&str>,
    mut on_event: impl FnMut(LcuEvent),
) -> Result<()> {
    let mut request = url.into_client_request()?;
    if let Some(auth) = auth_header {
        request
            .headers_mut()
            .insert(AUTHORIZATION, HeaderValue::from_str(auth)?);
    }

    // The client serves a self-signed Riot certificate on loopback only.
    let tls = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()?;
    let (mut socket, _) = tokio_tungstenite::connect_async_tls_with_config(
        request,
        None,
        false,
        Some(Connector::NativeTls(tls)),
    )
    .await
    .map_err(|err| anyhow!("lcu websocket connect failed: {err}"))?;

    let subscribe = serde_json::json!([WAMP_SUBSCRIBE, ALL_JSON_EVENTS]).to_string();
    socket.send(Message::Text(subscribe)).await?;

    while let Some(message) = socket.next().await {
        match message? {
            Message::Text(text) => {
                if let Some(event) = parse_event_frame(&text) {
                    on_event(event);
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gameflow_phase_event() {
        let frame = r#"[8,"OnJsonApiEvent",{"data":"ChampSelect","eventType":"Update","uri":"/lol-gameflow/v1/gameflow-phase"}]"#;
        let event = parse_event_frame(frame).expect("relevant event");
        assert_eq!(event.kind, LcuEventKind::GameflowPhase);
        assert_eq!(event.event_type, "Update");
        assert_eq!(event.data, Value::String("ChampSelect".into()));
    }

    #[test]
    fn ignores_unrelated_and_malformed_frames() {
        assert!(parse_event_frame(
            r#"[8,"OnJsonApiEvent",{"data":{},"eventType":"Update","uri":"/lol-chat/v1/me"}]"#
        )
        .is_none());
        assert!(parse_event_frame(r#"[5,"OnJsonApiEvent"]"#).is_none());
        assert!(parse_event_frame("").is_none());
        assert!(parse_event_frame("not json").is_none());
    }

    #[tokio::test]
    async fn subscribes_and_delivers_events_from_a_server() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let first = ws.next().await.unwrap().unwrap();
            ws.send(Message::Text(
                r#"[8,"OnJsonApiEvent",{"data":{},"eventType":"Update","uri":"/lol-chat/v1/me"}]"#.into(),
            ))
            .await
            .unwrap();
            ws.send(Message::Text(
                r#"[8,"OnJsonApiEvent",{"data":{"state":"InProgress"},"eventType":"Update","uri":"/lol-matchmaking/v1/ready-check"}]"#.into(),
            ))
            .await
            .unwrap();
            ws.close(None).await.unwrap();
            first.into_text().unwrap()
        });

        let mut seen = Vec::new();
        listen_at(&format!("ws://127.0.0.1:{port}/"), Some("Basic dGVzdA=="), |e| {
            seen.push(e)
        })
        .await
        .unwrap();

        assert_eq!(server.await.unwrap(), r#"[5,"OnJsonApiEvent"]"#);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].kind, LcuEventKind::ReadyCheck);
    }
}
