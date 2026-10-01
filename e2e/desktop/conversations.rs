use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_thread_sidebar_streaming_history_crud_and_errors() {
    let backend = Backend::start().await;
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready").await;
    assert!(
        iced_test::simulator(ui.emulator.as_ref().unwrap().view(&ui.program))
            .find("Ready · conversations saved locally")
            .is_err()
    );
    let first = backend.threads().await;
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    let id = first["items"][0]["id"].as_str().unwrap();
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"Hello from the desktop\"").await;
    ui.step("click \"Send ↑\"").await;
    ui.wait("SOLMU · streaming").await;
    ui.wait("Hello").await;
    assert_eq!(
        backend.messages(id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ui.wait("Hello from Solmu").await;
    ui.wait("Ready").await;
    ui.step("click \"Status\"").await;
    let status_thread = &backend.threads().await["items"][0];
    ui.wait(&format!(
        "Connected · {} · Default model · {}",
        status_thread["title"].as_str().unwrap(),
        status_thread["workspace"].as_str().unwrap()
    ))
    .await;
    ui.step("click \"Context\"").await;
    ui.wait(&format!(
        "2 messages · 0 tool calls · 0 skills · 0 MCP servers · 0 plugins\nWorkspace: {}",
        status_thread["workspace"].as_str().unwrap()
    ))
    .await;
    ui.step("click \"Export\"").await;
    let export = format!("Solmu-{id}.md");
    ui.step("click \"Save conversation\"").await;
    ui.wait("Conversation exported").await;
    assert!(
        std::fs::read_to_string(&export)
            .unwrap()
            .contains("Hello from Solmu")
    );
    std::fs::remove_file(export).unwrap();
    ui.step("click \"Copy reply\"").await;
    ui.step("click (460, 52)").await;
    for _ in 0..16 {
        ui.step("type backspace").await;
    }
    ui.step("type \"Desktop planning\"").await;
    ui.step("click \"Rename\"").await;
    ui.wait("Desktop planning").await;
    ui.wait("Ready").await;
    ui.step("click \"+\"").await;
    ui.wait("A little space for your\nnext big idea.").await;
    ui.wait("Ready").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        2
    );
    ui.step("click \"Desktop planning\"").await;
    ui.wait("Hello from the desktop").await;
    ui.wait("Ready").await;
    ui.step("click \"Delete\"").await;
    ui.wait("A little space for your\nnext big idea.").await;
    ui.wait("Ready").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
    ui.step("click \"New conversation\"").await;
    ui.wait("Ready").await;
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"FAIL\"").await;
    ui.step("type enter").await;
    ui.wait("LLM request failed; check provider credentials, model, and endpoint")
        .await;
    backend.create_thread("From another client").await;
    ui.wait("From another client").await;
    ui.wait("FAIL").await;
    ui.wait_enabled("From another client").await;
    ui.step("click \"From another client\"").await;
    ui.wait("Ready").await;
    let stop_id = backend.threads().await["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|thread| thread["title"] == "From another client")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"A new idea\"").await;
    ui.step("type enter").await;
    ui.wait("Hello").await;
    ui.step("click \"Stop ■\"").await;
    ui.wait("Ready").await;
    assert_eq!(
        backend.messages(&stop_id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"Hello from the desktop\"").await;
    ui.step("type enter").await;
    ui.wait("Hello from Solmu").await;
    ui.wait("Ready").await;
    // iced_test::Emulator::screenshot consumes its layout cache. Capture last.
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"A little more to explore\"").await;
    ui.wait_enabled("Send ↑").await;
    {
        let screenshot =
            ui.emulator
                .as_mut()
                .unwrap()
                .screenshot(&ui.program, &solmu_desktop::theme(), 1.0);
        for expected in [
            [250, 251, 248],
            [240, 243, 236],
            [225, 233, 220],
            [241, 244, 237],
            [39, 101, 81],
        ] {
            let count = screenshot
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[..3] == expected)
                .count();
            assert!(
                count > 100,
                "Desktop must render the web palette color {expected:?}; found {count} pixels"
            );
        }
        if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_none() {
            return;
        }
        let path = solmu_e2e::support::root().join("docs/screenshots/desktop.png");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
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
}
