use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_profile_edits_prompt_model_and_tracks_live_changes() {
    let backend = Backend::start().await;
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready · conversations saved locally").await;
    ui.step("click \"+\"").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
    ui.step("click \"Profile\"").await;
    ui.wait("Profile ready").await;
    ui.wait("test-model (default)").await;
    let original: serde_json::Value = backend
        .client
        .get(backend.endpoint("/api/v1/profile"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    ui.wait(&format!(
        "Edited on {}",
        original["edited_at"].as_str().unwrap()
    ))
    .await;
    ui.step("click (980, 230)").await;
    ui.step("type \"Use Finnish. \"").await;
    ui.step("click \"test-model (default)\"").await;
    ui.step("type \"gpt-6-luna\"").await;
    ui.step("click \"Save profile\"").await;
    ui.wait("Profile saved · changes apply to subsequent replies")
        .await;
    let stored: serde_json::Value = backend
        .client
        .get(backend.endpoint("/api/v1/profile"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        stored["system_prompt"],
        "Use Finnish. You are Solmu, an autonomous agent."
    );
    assert_eq!(stored["model"], "gpt-6-luna");
    assert_ne!(stored["edited_at"], original["edited_at"]);
    ui.wait(&format!(
        "Edited on {}",
        stored["edited_at"].as_str().unwrap()
    ))
    .await;
    backend
        .client
        .put(backend.endpoint("/api/v1/profile"))
        .json(&serde_json::json!({"system_prompt":"Changed elsewhere", "model":null}))
        .send()
        .await
        .unwrap();
    ui.wait("Profile ready").await;
    ui.step("click \"test-model (default)\"").await;
    ui.step("type \"my-unsaved-model\"").await;
    backend
        .client
        .put(backend.endpoint("/api/v1/profile"))
        .json(&serde_json::json!({"system_prompt":"Another update"}))
        .send()
        .await
        .unwrap();
    ui.wait("Profile changed elsewhere. Your draft is unchanged.")
        .await;
    ui.step("click \"Back to conversation\"").await;
    ui.wait("A little space for your\nnext big idea.").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        ui.step("click \"Profile\"").await;
        ui.wait("Profile ready").await;
        let screenshot =
            ui.emulator
                .as_mut()
                .unwrap()
                .screenshot(&ui.program, &solmu_desktop::theme(), 1.0);
        let path = solmu_e2e::support::root().join("docs/screenshots/desktop-profile.png");
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
