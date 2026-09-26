use super::*;

fn pane_header(tui: &Terminal, id: u64) -> (usize, usize) {
    let capture = tui.screen.lock().unwrap();
    let screen = capture.screen();
    for (row, text) in screen.rows(0, screen.size().1).enumerate() {
        if let Some(column) = text.find(&format!("Solmu {id} ·")) {
            return (row, text[..column].chars().count());
        }
    }
    panic!("Missing pane header for {id}: {}", screen.contents());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nested_layouts_focus_zoom_swap_resize_and_remember_each_tab() {
    let backend = Backend::start().await;
    let mut tui = Terminal::start(&backend, &[]);
    tui.wait("Solmu 1 · idle").await;
    tui.wait("Ready").await;
    tui.prefix('v');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready").await;
    tui.prefix('-');
    tui.wait("Solmu 3 · idle").await;
    tui.wait("Ready").await;
    tui.wait("Tabs: 1").await;
    let first = pane_header(&tui, 1);
    let second = pane_header(&tui, 2);
    let third = pane_header(&tui, 3);
    assert!(first.1 < second.1);
    assert_eq!(first.0, second.0);
    assert!(second.0 < third.0);
    assert_eq!(second.1, third.1);
    tui.command("/rename Lower right");
    tui.wait("SOLMU    Lower right").await;
    tui.wait("Ready").await;
    tui.prefix('k');
    tui.wait("› Solmu 2 · idle").await;
    tui.command("/rename Upper right");
    tui.wait("SOLMU    Upper right").await;
    tui.wait("Ready").await;
    tui.prefix('h');
    tui.wait("› Solmu 1 · idle").await;
    tui.command("/rename Left pane");
    tui.wait("SOLMU    Left pane").await;
    tui.wait("Ready").await;
    tui.prefix('z');
    tui.wait("zoomed").await;
    tui.wait_absent("Solmu 2 · idle").await;
    tui.wait_absent("Solmu 3 · idle").await;
    tui.click("Zoom").await;
    tui.wait_absent("zoomed").await;
    tui.wait("Solmu 3 · idle").await;
    let before_swap = pane_header(&tui, 1);
    tui.prefix('L');
    tokio::time::timeout(Duration::from_secs(5), async {
        while pane_header(&tui, 1).1 <= before_swap.1 {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .expect("Swap must move the live panes");
    let first = pane_header(&tui, 1);
    let second = pane_header(&tui, 2);
    let third = pane_header(&tui, 3);
    assert!(second.1 == before_swap.1 || third.1 == before_swap.1);
    tui.wait("› Solmu 1 · idle").await;
    tui.prefix('r');
    tui.wait("Resize:").await;
    tui.send(b"\x1b[C");
    tokio::time::timeout(Duration::from_secs(5), async {
        while pane_header(&tui, 1).1 <= first.1 {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    tui.send(b"\r");
    tui.wait_absent("Resize:").await;
    tui.prefix('n');
    tui.wait("Solmu 4 · idle").await;
    tui.wait("Ready").await;
    tui.wait("Tabs: 2").await;
    tui.prefix('1');
    tui.wait("SOLMU    Lower right").await;
    tui.wait("SOLMU    Upper right").await;
    tui.wait("SOLMU    Left pane").await;
    tui.prefix('x');
    tui.wait_absent("Solmu 1 · idle").await;
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Solmu 3 · idle").await;
    tui.close_tab(1);
    tui.wait_absent("Solmu 2 · idle").await;
    tui.wait_absent("Solmu 3 · idle").await;
    tui.wait("Solmu 4 · idle").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        4,
        "Closing layouts must preserve conversations"
    );
    tui.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mouse_context_menu_drag_and_close_manage_real_panes() {
    let backend = Backend::start().await;
    let mut tui = Terminal::start(&backend, &[]);
    tui.wait("Solmu 1 · idle").await;
    tui.wait("Ready").await;
    tui.send(b"\x1b[<2;40;8M\x1b[<2;40;8m");
    tui.wait("Split down").await;
    tui.click("Split right").await;
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready").await;
    let (_, right) = pane_header(&tui, 2);
    // The dedicated divider is two cells to the left of the label's first
    // letter: the pane begins with a focus marker and a space.
    let divider = right - 2;
    tui.send(
        format!(
            "\x1b[<0;{};12M\x1b[<32;{};12M\x1b[<0;{};12m",
            divider,
            divider + 20,
            divider + 20
        )
        .as_bytes(),
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while pane_header(&tui, 2).1 <= right + 10 {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    tui.send(b"\x1b[<2;155;10M\x1b[<2;155;10m");
    tui.wait("Split down").await;
    tui.click("Split down").await;
    tui.wait("Solmu 3 · idle").await;
    tui.wait("Ready").await;
    let third = pane_header(&tui, 3);
    let second = pane_header(&tui, 2);
    assert!(third.0 > second.0);
    tui.send(
        format!(
            "\x1b[<2;{};{}M\x1b[<2;{};{}m",
            third.1 + 6,
            third.0 + 3,
            third.1 + 6,
            third.0 + 3
        )
        .as_bytes(),
    );
    tui.wait("Zoom / restore").await;
    tui.click("Zoom / restore").await;
    tui.wait("zoomed").await;
    tui.wait_absent("Solmu 2 · idle").await;
    tui.click("Zoom").await;
    tui.wait("Solmu 2 · idle").await;
    tui.send(
        format!(
            "\x1b[<2;{};{}M\x1b[<2;{};{}m",
            third.1 + 6,
            third.0 + 3,
            third.1 + 6,
            third.0 + 3
        )
        .as_bytes(),
    );
    tui.click("Close pane").await;
    tui.wait_absent("Solmu 3 · idle").await;
    tui.wait("Solmu 1 · idle").await;
    tui.wait("Solmu 2 · idle").await;
    solmu_e2e::support::capture_terminal(tui.screen.lock().unwrap().screen(), "muxer");
    tui.exit().await;
}
