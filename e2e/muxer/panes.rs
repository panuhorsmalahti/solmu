use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_solmu_panes_stream_switch_split_resize_and_keep_conversation_features() {
    let backend = Backend::start().await;
    let mut tui = Terminal::start(&backend, &[]);
    tui.wait("Ready ·").await;
    tui.command("/rename First workspace");
    tui.wait("SOLMU    First workspace").await;
    tui.wait("Ready ·").await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    tui.command("Hello from pane one");
    tui.wait("Solmu 1 · working").await;
    tui.wait("SOLMU · streaming").await;
    tui.prefix('n');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready ·").await;
    tui.command("/ren\tSecond workspace");
    tui.wait("SOLMU    Second workspace").await;
    tui.wait("Ready ·").await;
    tui.prefix('1');
    tui.wait("Hello from Solmu").await;
    tui.wait("Ready ·").await;
    assert_eq!(
        backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    tui.prefix('s');
    tui.wait("SOLMU    Second workspace").await;
    tui.wait("SOLMU    First workspace").await;
    solmu_e2e::support::capture_terminal(tui.screen.lock().unwrap().screen(), "muxer");
    {
        let parser = tui.screen.lock().unwrap();
        let screen = parser.screen();
        let (rows, columns) = screen.size();
        let colored = (0..rows)
            .flat_map(|row| (0..columns).map(move |column| (row, column)))
            .filter(|(row, column)| {
                screen.cell(*row, *column).is_some_and(|cell| {
                    cell.bgcolor() != vt100::Color::Default
                        || cell.fgcolor() != vt100::Color::Default
                })
            })
            .count();
        assert!(colored > 100, "Muxer must render its theme colors");
    }
    tui.resize(44, 200);
    tui.wait_resized(200).await;
    tui.wait("SOLMU    First workspace").await;
    tui.wait("SOLMU    Second workspace").await;
    // Confirm native resize/focus transitions before sending the next action.
    tui.send(b"\x1b[<0;160;6M\x1b[<0;160;6m");
    tui.wait("› Solmu 2").await;
    tui.send(b"\x1b[<0;30;3M\x1b[<0;30;3m");
    tui.wait("› Solmu 1").await;
    tui.send(b"\x1b[<0;55;3M\x1b[<0;55;3m");
    tui.wait("› Solmu 2").await;
    tui.command("/rename Clicked workspace");
    tui.wait("SOLMU    Clicked workspace").await;
    tui.prefix('s');
    tui.command(&format!("/open {id}"));
    tui.wait("Hello from pane one").await;
    tui.wait("Ready ·").await;
    tui.command("/threads");
    tui.wait("Conversations ·").await;
    tui.wait(&id).await;
    tui.wait("Ready ·").await;
    tui.command("/new Third conversation");
    tui.wait("SOLMU    Third conversation").await;
    tui.wait("Ready ·").await;
    tui.command("/delete");
    tui.wait("No conversation").await;
    tui.wait("Ready ·").await;
    tui.command("/exit");
    tui.wait("Solmu 2 · exited 0").await;
    tui.prefix('r');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready ·").await;
    tui.prefix('x');
    tui.wait("SOLMU    First workspace").await;
    assert!(!tui.contents().contains("Solmu 2 ·"));
    tui.exit().await;
    assert_eq!(
        backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
