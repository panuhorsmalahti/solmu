use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mouse_controls_group_tabs_by_space_preserve_focus_and_close_sessions() {
    let backend = Backend::start().await;
    let other = backend.directory.path().join("Another project");
    std::fs::create_dir(&other).unwrap();
    let mut tui = Terminal::start(&backend, &[]);
    tui.wait("Ready ·").await;
    tui.command("/rename First tab");
    tui.wait("SOLMU    First tab").await;
    tui.wait("Ready ·").await;
    tui.click("+ Tab").await;
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready ·").await;
    tui.wait("2 tabs").await;
    tui.command("/rename Second tab");
    tui.wait("SOLMU    Second tab").await;
    tui.wait("Ready ·").await;
    tui.click("Solmu 1").await;
    tui.wait("SOLMU    First tab").await;
    tui.click("+ Space").await;
    tui.wait("Workspace path:").await;
    tui.click("Cancel").await;
    tui.wait_absent("New workspace").await;
    tui.click("+ Space").await;
    tui.wait("Workspace path:").await;
    tui.send(format!("\x1b[200~{}\x1b[201~", other.display()).as_bytes());
    tui.wait("Another project").await;
    tui.click("Create").await;
    tui.wait("Solmu 3 · idle").await;
    tui.wait("Ready ·").await;
    tui.click("+ Tab").await;
    tui.wait("Solmu 4 · idle").await;
    tui.wait("Ready ·").await;
    tui.command("/model gpt-6-luna");
    tui.wait("Model: gpt-6-luna").await;
    tui.wait("Ready ·").await;
    let threads = backend.threads().await;
    assert_eq!(threads["items"].as_array().unwrap().len(), 4);
    let model_thread = threads["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|thread| thread["model"] == "gpt-6-luna")
        .unwrap();
    assert_eq!(
        std::path::PathBuf::from(model_thread["workspace"].as_str().unwrap()),
        other.canonicalize().unwrap()
    );
    tui.click_at(3, 4);
    tui.wait("SOLMU    First tab").await;
    tui.close_tab(2);
    tui.wait_absent("Solmu 2").await;
    tui.wait("SOLMU    First tab").await;
    tui.close_tab(1);
    tui.wait_absent("Solmu 1").await;
    tui.wait("Model: gpt-6-luna").await;
    tui.wait("› Solmu 4").await;
    tui.click("Split").await;
    tui.wait("Solmu 3 · idle").await;
    tui.command("/profile");
    tui.wait("SOLMU / PROFILE").await;
    tui.wait("Edited on ").await;
    tui.send(b"\x1b");
    tui.wait_absent("SOLMU / PROFILE").await;
    tui.command("/exit");
    tui.wait("Solmu 4 · exited 0").await;
    tui.click("Restart").await;
    tui.wait("Solmu 4 · idle").await;
    tui.wait("Ready ·").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        5
    );
    tui.close_tab(4);
    tui.wait_absent("Solmu 4").await;
    tui.wait("Solmu 3 · idle").await;
    tui.close_tab(3);
    tui.wait_exit().await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        5,
        "Closing tabs keeps saved threads"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn space_tab_limit_and_small_terminal_resize_keep_the_tui_usable() {
    let backend = Backend::start().await;
    let mut tui = Terminal::start(&backend, &[]);
    tui.wait("Solmu 1 · idle").await;
    for id in 2..=8 {
        tui.click("+ Tab").await;
        tui.wait(&format!("Solmu {id} · idle")).await;
    }
    tui.wait("8 tabs").await;
    tui.click("+ Tab").await;
    tui.wait("Eight tabs are already open").await;
    tui.resize(18, 60);
    tui.wait_resized(60).await;
    tui.wait("› Solmu 8").await;
    tui.resize(40, 180);
    tui.wait_resized(180).await;
    tui.close_tab(8);
    tui.wait_absent("Solmu 8").await;
    tui.click("+ Tab").await;
    tui.wait("Solmu 9 · idle").await;
    tui.exit().await;
}
