use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slash_command_autocomplete_opens_plugins_mcp_and_skills() {
    let backend = Backend::start().await;
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready").await;
    ui.step("click \"Message Solmu…\"").await;

    ui.step("type \"/pl\"").await;
    ui.wait("Show installed plugins").await;
    ui.step("click \"/plugins\"").await;
    ui.step("click \"Send ↑\"").await;
    ui.wait("No plugins installed in this workspace.").await;
    ui.step("click \"Back to conversation\"").await;

    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"/mcp\"").await;
    ui.wait("Show MCP servers and tools").await;
    ui.step("click \"/mcp\"").await;
    ui.step("click \"Send ↑\"").await;
    ui.wait("No MCP servers configured in this workspace.")
        .await;
    ui.step("click \"Back to conversation\"").await;

    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"/skills\"").await;
    ui.wait("Show workspace skills").await;
    ui.step("click \"/skills\"").await;
    ui.step("click \"Send ↑\"").await;
    ui.wait("No skills installed in this workspace.").await;
}
