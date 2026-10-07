use super::*;

async fn ask_solmu_to_remember(backend: &Backend, content: &str) -> String {
    let thread = backend
        .client
        .post(backend.endpoint("/api/v1/threads"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let id = thread["id"].as_str().unwrap();
    let message = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/messages")))
        .json(&serde_json::json!({
            "content": format!(
                "TOOLS {}",
                serde_json::json!([{
                    "name": "Memory",
                    "arguments": {"action": "write", "content": content}
                }])
            )
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let response = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/responses")))
        .json(&serde_json::json!({"message_id": message["id"]}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    response.text().await.unwrap();
    id.to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn memories_page_lists_saved_memories_read_only() {
    let backend = Backend::start().await;
    for content in ["My cat is named Miso", "Miso likes salmon"] {
        let thread = ask_solmu_to_remember(&backend, content).await;
        backend
            .client
            .delete(backend.endpoint(&format!("/api/v1/threads/{thread}")))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
    }
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready").await;
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"/memories\"").await;
    ui.step("click \"/memories\"").await;
    ui.step("click \"Send ↑\"").await;
    ui.wait("My cat is named Miso").await;
    ui.wait("Miso likes salmon").await;
    let mut simulator = iced_test::simulator(ui.emulator.as_ref().unwrap().view(&ui.program));
    assert!(simulator.find("Add memory").is_err());
    assert!(simulator.find("Edit").is_err());
    drop(simulator);
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        let screenshot =
            ui.emulator
                .as_mut()
                .unwrap()
                .screenshot(&ui.program, &solmu_desktop::theme(), 1.0);
        let file = std::fs::File::create(
            solmu_e2e::support::root().join("docs/screenshots/desktop-memories.png"),
        )
        .unwrap();
        let mut encoder = png::Encoder::new(file, screenshot.size.width, screenshot.size.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&screenshot.rgba)
            .unwrap();
    }
}
