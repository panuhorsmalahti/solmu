use super::state::AppState;
use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
};
use tokio::sync::broadcast;

pub async fn connect(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> Response {
    let events = state.events.subscribe();
    upgrade.on_upgrade(move |socket| notifications(socket, events))
}

async fn notifications(
    mut socket: WebSocket,
    mut events: broadcast::Receiver<super::state::Change>,
) {
    if socket
        .send(Message::Text("{\"type\":\"ready\"}".into()))
        .await
        .is_err()
    {
        return;
    }
    loop {
        tokio::select! {
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Ping(bytes))) => { if socket.send(Message::Pong(bytes)).await.is_err() { break; } },
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {},
            },
            change = events.recv() => {
                let change = match change {
                    Ok(change) => change,
                    Err(broadcast::error::RecvError::Lagged(_)) => AppState::resync(),
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if socket.send(Message::Text(serde_json::to_string(&change).expect("notification JSON").into())).await.is_err() { break; }
            }
        }
    }
}
