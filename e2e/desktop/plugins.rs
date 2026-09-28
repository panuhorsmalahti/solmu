use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_plugins_update_automatically_and_preserve_draft() {
    let backend = Backend::start().await;
    let workspace = backend.directory.path().join("workspace");
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready").await;
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"Keep my unsent plugin draft\"").await;
    ui.step("click \"Plugins\"").await;
    ui.wait("No plugins installed in this workspace.").await;
    let root = solmu_e2e::support::plugins::install(&workspace, false);
    ui.wait("Tools for this project").await;
    std::fs::write(root.join("plugin.json"), "invalid JSON").unwrap();
    ui.wait("expected value at line 1 column 1").await;
    solmu_e2e::support::plugins::install(&workspace, false);
    ui.wait("Tools for this project").await;
    ui.step("click \"Back to conversation\"").await;
    ui.wait("Keep my unsent plugin draft").await;
    ui.wait_enabled("Plugins").await;
    ui.step("click \"Plugins\"").await;
    ui.wait("Tools for this project").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        let screenshot =
            ui.emulator
                .as_mut()
                .unwrap()
                .screenshot(&ui.program, &solmu_desktop::theme(), 1.0);
        let file = std::fs::File::create(
            solmu_e2e::support::root().join("docs/screenshots/desktop-plugins.png"),
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
