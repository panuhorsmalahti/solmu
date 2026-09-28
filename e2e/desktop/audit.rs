use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_audit_pages_expands_calls_and_updates_without_losing_draft() {
    let backend = Backend::start().await;
    let thread = solmu_e2e::support::audit::seed(&backend, 5).await;
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready").await;
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"Keep my draft\"").await;
    ui.step("click \"Audit\"").await;
    ui.wait("Page 1").await;
    ui.wait("40.0%").await;
    ui.wait("Grep · completed").await;
    ui.step("click \"Grep · completed\"").await;
    ui.wait("Arguments").await;
    ui.wait("Result").await;
    ui.wait_enabled("Next").await;
    ui.step("click \"Next\"").await;
    ui.wait("Page 2").await;
    ui.wait_enabled("Previous").await;
    ui.step("click \"Previous\"").await;
    ui.wait("Page 1").await;
    let message = backend
        .send_message(
            &thread,
            "TOOLS [{\"name\":\"Read\",\"arguments\":{\"path\":\"missing-audit.txt\"}}]",
        )
        .await;
    let response = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{thread}/responses")))
        .json(&serde_json::json!({"message_id":message["id"]}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert!(response.text().await.unwrap().contains("event: done"));
    ui.wait("Read · failed").await;
    ui.step("click \"Back to conversation\"").await;
    ui.wait("Keep my draft").await;
    ui.wait_enabled("Audit").await;
    ui.step("click \"Audit\"").await;
    ui.wait("Read · failed").await;
    ui.step("click \"Read · failed\"").await;
    ui.wait("Arguments").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        let screenshot =
            ui.emulator
                .as_mut()
                .unwrap()
                .screenshot(&ui.program, &solmu_desktop::theme(), 1.0);
        let file = std::fs::File::create(
            solmu_e2e::support::root().join("docs/screenshots/desktop-audit.png"),
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
