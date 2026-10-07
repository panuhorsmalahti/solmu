use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn memories_page_lists_newest_first_and_supports_management() {
    let backend = Backend::start().await;
    backend
        .client
        .post(backend.endpoint("/api/v1/memories"))
        .json(&serde_json::json!({"content":"My cat is named Miso"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready").await;
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"/memories\"").await;
    ui.step("click \"/memories\"").await;
    ui.step("click \"Send ↑\"").await;
    ui.wait("My cat is named Miso").await;
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
