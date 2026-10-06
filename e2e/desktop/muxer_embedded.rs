use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn muxer_embedded_hides_header_controls_but_keeps_chat_commands() {
    let backend = Backend::start().await;
    let mut ui = tokio::task::block_in_place(|| {
        Ui::new(solmu_desktop::application_for_muxer(Api::new(&backend.url)))
    });
    ui.wait("Ready").await;

    let mut simulator = iced_test::simulator(ui.emulator.as_ref().unwrap().view(&ui.program));
    for hidden in [
        "Plugins",
        "MCP",
        "Skills",
        "Conversation title",
        "Rename",
        "Delete",
    ] {
        assert!(
            simulator.find(hidden).is_err(),
            "unexpected embedded control: {hidden}"
        );
    }
    drop(simulator);

    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"/pl\"").await;
    ui.wait("Show installed plugins").await;
}
