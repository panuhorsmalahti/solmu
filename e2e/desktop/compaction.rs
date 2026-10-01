use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_can_compact_conversation_history() {
    let backend = Backend::start().await;
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready").await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    for (index, prompt) in ["First desktop turn", "Second desktop turn"]
        .into_iter()
        .enumerate()
    {
        ui.step("click \"Message Solmu…\"").await;
        ui.step(&format!("type \"{prompt}\"")).await;
        ui.step("type enter").await;
        let expected_messages = (index + 1) * 2;
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if backend.messages(&id).await["items"]
                    .as_array()
                    .unwrap()
                    .len()
                    == expected_messages
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("Desktop did not persist {expected_messages} messages"));
        ui.wait("Hello from Solmu").await;
        ui.wait("Ready").await;
    }
    ui.step("click \"Compact\"").await;
    ui.wait("Conversation summary (compacted):\n\nA new idea")
        .await;
    ui.wait("Ready").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        let screenshot =
            ui.emulator
                .as_mut()
                .unwrap()
                .screenshot(&ui.program, &solmu_desktop::theme(), 1.0);
        let file =
            std::fs::File::create(solmu_e2e::support::root().join("docs/screenshots/desktop.png"))
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
    assert_eq!(
        backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
