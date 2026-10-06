use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_main_screen_screenshot() {
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_none() {
        return;
    }
    let backend = Backend::start().await;
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready").await;
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"Hello from the desktop\"").await;
    ui.step("type enter").await;
    ui.wait("Hello from Solmu").await;
    ui.wait("Ready").await;

    let screenshot =
        ui.emulator
            .as_mut()
            .unwrap()
            .screenshot(&ui.program, &solmu_desktop::theme(), 1.0);
    let path = solmu_e2e::support::root().join("docs/screenshots/desktop.png");
    let mut encoder = png::Encoder::new(
        std::fs::File::create(path).unwrap(),
        screenshot.size.width,
        screenshot.size.height,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&screenshot.rgba)
        .unwrap();
}
