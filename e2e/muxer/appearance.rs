use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspace_chrome_marks_selected_space_tab_and_pane_without_changing_mouse_controls() {
    let backend = Backend::start().await;
    let mut tui = Terminal::start(&backend, &[]);
    tui.wait("Ready").await;
    tui.wait("WORKSPACES  01").await;
    tui.wait("SOLMU MUXER").await;
    {
        let capture = tui.screen.lock().unwrap();
        let screen = capture.screen();
        assert_eq!(
            screen.cell(3, 26).unwrap().contents(),
            "━",
            "selected tab should have an accent underline"
        );
        assert_eq!(
            screen.cell(3, 26).unwrap().fgcolor(),
            vt100::Color::Rgb(169, 223, 185)
        );
        assert_eq!(
            screen.cell(4, 26).unwrap().bgcolor(),
            vt100::Color::Rgb(48, 84, 73),
            "focused pane header should stand apart from terminal content"
        );
        #[cfg(windows)]
        assert!(!screen.rows(0, 1).next().unwrap().contains("\\\\?\\"));
    }
    tui.click("+ Tab").await;
    tui.wait("Tabs: 2").await;
    tui.wait("Solmu 2 · idle").await;
    tui.exit().await;
}
