use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_manages_scheduled_tasks_and_updates_from_other_clients() {
    let backend = Backend::start().await;
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready").await;
    ui.step("click \"Tasks\"").await;
    ui.wait("No tasks yet.").await;
    ui.step("click \"Name\"").await;
    ui.step("type \"Morning review\"").await;
    ui.step("click \"What should Solmu do?\"").await;
    ui.step("type \"Summarize the workspace\"").await;
    ui.step("click \"Cron\"").await;
    ui.step("click \"0 9 * * * (UTC)\"").await;
    ui.step("type \"0 9 * * *\"").await;
    ui.step("click \"Create task\"").await;
    ui.wait("Morning review").await;
    ui.wait("Run history").await;
    ui.step("click \"Run history\"").await;
    ui.wait("No runs yet.").await;
    ui.step("click \"Pause\"").await;
    ui.wait("Resume").await;
    ui.step("click \"Resume\"").await;
    ui.wait("Pause").await;
    ui.wait_enabled("Run now").await;
    ui.step("click \"Run now\"").await;
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let tasks: serde_json::Value = backend
                .client
                .get(backend.endpoint("/api/v1/tasks"))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if tasks["items"][0]["last_status"] == "completed" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    ui.wait_enabled("Run now").await;
    ui.step("click \"Edit\"").await;
    ui.wait("Edit task").await;
    ui.step("click \"Save task\"").await;
    ui.wait("New task").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        let screenshot =
            ui.emulator
                .as_mut()
                .unwrap()
                .screenshot(&ui.program, &solmu_desktop::theme(), 1.0);
        let file = std::fs::File::create(
            solmu_e2e::support::root().join("docs/screenshots/desktop-tasks.png"),
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_deletes_scheduled_task_from_live_list() {
    let backend = Backend::start().await;
    let created = backend.client.post(backend.endpoint("/api/v1/tasks"))
        .json(&serde_json::json!({"name":"Temporary task","prompt":"Say hello","schedule_kind":"cron","schedule":"0 9 * * *"}))
        .send().await.unwrap();
    assert!(created.status().is_success());
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready").await;
    ui.step("click \"Tasks\"").await;
    ui.wait("Temporary task").await;
    ui.wait_enabled("Delete").await;
    ui.step("click \"Delete\"").await;
    ui.wait("No tasks yet.").await;
}
