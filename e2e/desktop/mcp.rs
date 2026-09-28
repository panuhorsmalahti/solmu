use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_mcp_status_updates_automatically_and_preserves_the_conversation_draft() {
    let backend = Backend::start().await;
    let workspace = backend.directory.path().join("workspace");
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready").await;
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"Keep my unsent MCP draft\"").await;
    ui.step("click \"MCP\"").await;
    ui.wait("No MCP servers configured in this workspace.")
        .await;
    let config = solmu_e2e::support::mcp::install(&workspace, false);
    ui.wait("Search project notes with MCP.").await;
    std::fs::write(&config, "invalid JSON").unwrap();
    ui.wait("MCP config must be valid JSON").await;
    ui.step("click \"Back to conversation\"").await;
    ui.wait("Keep my unsent MCP draft").await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    solmu_e2e::support::mcp::install(&workspace, false);
    ui.step("click \"MCP\"").await;
    ui.wait("Search project notes with MCP.").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        let screenshot =
            ui.emulator
                .as_mut()
                .unwrap()
                .screenshot(&ui.program, &solmu_desktop::theme(), 1.0);
        let file = std::fs::File::create(
            solmu_e2e::support::root().join("docs/screenshots/desktop-mcp.png"),
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
