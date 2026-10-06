use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_slash_command_creates_and_lists_goals() {
    let backend = Backend::start().await;
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready").await;
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"/goal Prepare the first launch\"").await;
    ui.step("click \"Send ↑\"").await;
    ui.wait("Goal started: Prepare the first launch").await;
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"/goal\"").await;
    ui.step("click \"Send ↑\"").await;
    ui.wait("Prepare the first launch").await;
}
