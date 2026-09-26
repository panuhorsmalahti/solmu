use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_model_picker_saves_and_clears_thread_overrides() {
    let backend = Backend::start().await;
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready · conversations saved locally").await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ui.step("click \"Default model\"").await;
    ui.wait("GPT 6 Luna").await;
    ui.wait_enabled("GPT 6 Luna").await;
    ui.step("click \"GPT 6 Luna\"").await;
    ui.wait("gpt-6-luna").await;
    ui.wait("Ready · conversations saved locally").await;
    let thread: serde_json::Value = backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{id}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(thread["model"], "gpt-6-luna");
    ui.step("click \"gpt-6-luna\"").await;
    ui.wait("GPT 6 Luna").await;
    ui.wait_enabled("Default model").await;
    ui.step("click \"Default model\"").await;
    ui.wait("Default model").await;
    ui.wait("Ready · conversations saved locally").await;
    let thread: serde_json::Value = backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{id}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(thread["model"].is_null());
}
