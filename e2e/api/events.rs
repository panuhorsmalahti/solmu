use super::*;

#[tokio::test]
async fn websocket_notifications_cover_thread_message_title_and_delete_changes() {
    let backend = Backend::start().await;
    let (mut socket, _) = tokio_tungstenite::connect_async(format!(
        "{}/api/v1/events",
        backend.url.replacen("http", "ws", 1)
    ))
    .await
    .unwrap();
    assert!(
        socket
            .next()
            .await
            .unwrap()
            .unwrap()
            .to_text()
            .unwrap()
            .contains("ready")
    );
    let thread = backend.create_thread("New conversation").await;
    let id = thread["id"].as_str().unwrap();
    let notification = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(notification.to_text().unwrap().contains(id));
    backend.send_message(id, "Name this thread").await;
    for _ in 0..2 {
        let notification = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            notification
                .to_text()
                .unwrap()
                .contains("conversation_changed")
        );
    }
    backend
        .client
        .delete(backend.endpoint(&format!("/api/v1/threads/{id}")))
        .send()
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .to_text()
            .unwrap()
            .contains(id)
    );
}
