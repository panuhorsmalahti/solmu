use super::sessions::Session;
use super::*;

fn config_path(backend: &Backend) -> std::path::PathBuf {
    let directory = backend.directory.path().join("muxer-state");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join("config.toml")
}
async fn palette(tui: &Terminal, row: u16, col: u16, foreground: bool, expected: vt100::Color) {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let actual = {
                let parser = tui.screen.lock().unwrap();
                let cell = parser.screen().cell(row, col).unwrap();
                if foreground {
                    cell.fgcolor()
                } else {
                    cell.bgcolor()
                }
            };
            if actual == expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("Expected theme color {expected:?}:\n{}", tui.contents()));
}
async fn edit(tui: &Terminal, name: &str) {
    if !tui.contents().contains("Automatic updates") {
        tui.click("Settings").await;
        tui.wait("Automatic updates").await;
    }
    tui.send(format!("\x15{name}").as_bytes());
    tui.wait(&format!("> {name}")).await;
    tui.send(b"\r");
    tui.wait("Edit setting").await;
}
async fn save(tui: &Terminal, value: &str) {
    tui.send(format!("\x15{value}\r").as_bytes());
    tui.wait_absent("Edit setting").await;
}
async fn close_settings(tui: &Terminal) {
    tui.send(b"\x1b");
    tui.wait_absent("Automatic updates").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hot_reload_changes_prefix_direct_shortcuts_theme_and_sidebar_without_losing_cli_drafts() {
    let backend = Backend::start().await;
    let path = backend.directory.path().join("personal.toml");
    std::fs::write(&path, "# Personal Muxer configuration\n").unwrap();
    let mut tui = Terminal::start_config(&backend, &path);
    tui.wait("Ready").await;
    tui.send(b"Draft stays");
    tui.wait("Draft stays").await;
    std::fs::write(
        &path,
        r##"[keys]
prefix = "ctrl+a"
new_tab = ["prefix+n", "ctrl+alt+n"]
[theme]
name = "light"
[theme.colors]
accent = "#135724"
[ui]
sidebar_visible = false
sidebar_width = 34
"##,
    )
    .unwrap();
    tui.wait("ctrl+a shortcuts").await;
    tui.wait_absent("solmu / muxer").await;
    tui.wait("Draft stays").await;
    palette(&tui, 0, 0, false, vt100::Color::Rgb(232, 238, 228)).await;
    // A direct legacy Alt+Ctrl+N chord needs no prefix.
    tui.send(b"\x1b\x0e");
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready").await;
    tui.send(b"\x01c");
    tui.wait("Unknown Muxer shortcut").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        2
    );
    tui.send(b"\x01n");
    tui.wait("Solmu 3 · idle").await;
    tui.wait("Ready").await;
    tui.send(b"\x01g");
    tui.wait("Find spaces, tabs and panes").await;
    tui.send(b"Solmu 1 idle\r");
    tui.wait_absent("Find spaces, tabs and panes").await;
    tui.wait("Draft stays").await;
    tui.send(" @ \\ 🧶".as_bytes());
    tui.wait("Draft stays @ \\ 🧶").await;
    tui.send(b"\x01?");
    tui.wait("Keyboard help").await;
    tui.send(b"new tab");
    tui.wait("> new tab").await;
    tui.wait("ctrl+a then n / ctrl+alt+n").await;
    tui.wait_absent("ctrl+b").await;
    tui.send(b"\x1b");
    tui.wait_absent("Keyboard help").await;
    std::fs::write(&path, "[theme]\nname = \"unknown\"\n").unwrap();
    tui.wait("Configuration error:").await;
    palette(&tui, 0, 0, false, vt100::Color::Rgb(232, 238, 228)).await;
    tui.wait("Draft stays").await;
    std::fs::write(
        &path,
        r##"[keys]
prefix = "ctrl+a"
[theme]
name = "solmu"
[theme.colors]
accent = "#246813"
[ui]
sidebar_width = 34
"##,
    )
    .unwrap();
    tui.wait_absent("Configuration error:").await;
    tui.wait("solmu / muxer").await;
    palette(&tui, 0, 2, true, vt100::Color::Rgb(36, 104, 19)).await;
    palette(&tui, 0, 30, false, vt100::Color::Rgb(29, 48, 49)).await;
    tui.wait("Draft stays").await;
    std::fs::remove_file(&path).unwrap();
    tui.wait("ctrl+b shortcuts").await;
    palette(&tui, 0, 2, true, vt100::Color::Rgb(169, 223, 185)).await;
    for thread in backend.threads().await["items"].as_array().unwrap() {
        assert!(
            backend.messages(thread["id"].as_str().unwrap()).await["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    tui.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_preserve_comments_merge_unrelated_edits_and_protect_conflicting_drafts() {
    let backend = Backend::start().await;
    let _session = Session {
        backend: &backend,
        name: "settings",
    };
    let path = config_path(&backend);
    std::fs::write(
        &path,
        "# Keep my comments\n[theme]\nname = \"solmu\" # Current theme\n",
    )
    .unwrap();
    let mut first = Terminal::start_session(&backend, "settings");
    first.wait("Ready").await;
    let mut second = Terminal::start_session(&backend, "settings");
    second.wait("Ready").await;
    edit(&first, "theme.name").await;
    first.send(b"\x15light");
    first.wait("light").await;
    edit(&second, "ui.sidebar_width").await;
    save(&second, "32").await;
    first.wait("Edit setting").await;
    first.wait("light").await;
    first.send(b"\r");
    first.wait_absent("Edit setting").await;
    let source = std::fs::read_to_string(&path).unwrap();
    assert!(source.contains("# Keep my comments"));
    assert!(source.contains("# Current theme"));
    assert!(source.contains("sidebar_width = 32"));
    assert!(source.contains("name = \"light\""));
    edit(&first, "theme.name").await;
    first.send(b"\x15terminal");
    edit(&second, "theme.name").await;
    save(&second, "solmu").await;
    first.wait("terminal").await;
    first.send(b"\r");
    first.wait("This setting changed in another terminal").await;
    first.wait("terminal").await;
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("name = \"solmu\"")
    );
    first.send(b"\x1b");
    first.wait_absent("Edit setting").await;
    first.wait("theme.name    solmu").await;
    edit(&second, "theme.colors.accent").await;
    save(&second, "#aabbcc").await;
    palette(&first, 0, 2, true, vt100::Color::Rgb(170, 187, 204)).await;
    edit(&second, "theme.colors.accent").await;
    save(&second, "").await;
    palette(&first, 0, 2, true, vt100::Color::Rgb(169, 223, 185)).await;
    assert!(!std::fs::read_to_string(&path).unwrap().contains("#aabbcc"));
    close_settings(&first).await;
    close_settings(&second).await;
    first.click("Sidebar").await;
    first.wait_absent("solmu / muxer").await;
    second.wait("solmu / muxer").await;
    first.prefix('B');
    first.wait("solmu / muxer").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
    first.exit().await;
    second.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_configuration_uses_defaults_and_unbound_actions_still_have_mouse_controls() {
    let backend = Backend::start().await;
    let path = config_path(&backend);
    std::fs::write(&path, "[keys]\nprefix = \"n\"\n").unwrap();
    let mut tui = Terminal::start(&backend, &[]);
    tui.wait("Ready").await;
    tui.wait("Configuration error:").await;
    tui.prefix('n');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready").await;
    for (source, error) in [
        ("[keys]\nnew_tab = \"prefix+g\"\n", "Duplicate shortcut"),
        (
            "[theme.colors]\naccent = \"not-a-color\"\n",
            "Invalid color",
        ),
        (
            "[server]\nheadless_columns = 0\n",
            "server.headless_columns must be an integer",
        ),
        (
            "[ui]\nsidebar_visble = true\n",
            "Unknown setting ui.sidebar_visble",
        ),
    ] {
        std::fs::write(&path, source).unwrap();
        tui.wait(error).await;
    }
    std::fs::write(&path, "[keys]\nnew_tab = []\n").unwrap();
    tui.wait_absent("Configuration error:").await;
    tui.prefix('?');
    tui.wait("Keyboard help").await;
    tui.send(b"new tab");
    tui.wait("> new tab").await;
    tui.wait("Unbound    New tab").await;
    tui.send(b"\x1b");
    tui.wait_absent("Keyboard help").await;
    tui.prefix('n');
    tui.wait("Unknown Muxer shortcut").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        2
    );
    tui.click("+ Tab").await;
    tui.wait("Solmu 3 · idle").await;
    tui.wait("Ready").await;
    std::fs::write(&path, "[keys]\nprefix = \"f12\"\n").unwrap();
    tui.wait("f12 shortcuts").await;
    tui.send(b"\x1b[24~n");
    tui.wait("Solmu 4 · idle").await;
    tui.wait("Ready").await;
    tui.send(b"\x1b[24~q");
    tui.wait_exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn headless_dimensions_and_cwd_policy_apply_to_new_panes_and_keep_existing_workspaces() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "cwd-policy",
    };
    let path = config_path(&backend);
    let project = path.parent().unwrap().join("Relative work");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(&path, "[server]\nheadless_columns = 90\nheadless_rows = 18\n[workspace]\nnew_cwd = \"path\"\npath = \"Relative work\"\n").unwrap();
    assert!(session.command(&["server", "start"]).status.success());
    let read = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let output = session.command(&["pane", "read", "1"]);
            let text = String::from_utf8(output.stdout).unwrap();
            if text.contains("Ready") {
                break text;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        read.lines().all(|line| line.chars().count() <= 62),
        "Headless screen must respect configured dimensions: {read}"
    );
    let first = backend.threads().await["items"][0].clone();
    assert_eq!(
        std::path::PathBuf::from(first["workspace"].as_str().unwrap()),
        project.canonicalize().unwrap()
    );
    std::fs::write(
        &path,
        "[workspace]\nnew_cwd = \"path\"\npath = \"Missing new workspace\"\n",
    )
    .unwrap();
    let mut tui = Terminal::start_session(&backend, "cwd-policy");
    tui.wait("Ready").await;
    std::fs::write(&path, "[workspace]\nnew_cwd = \"current\"\n").unwrap();
    edit(&tui, "workspace.new_cwd").await;
    tui.wait("current").await;
    tui.send(b"\x1b");
    tui.wait_absent("Edit setting").await;
    close_settings(&tui).await;
    tui.prefix('n');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready").await;
    let threads = backend.threads().await;
    let second = threads["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|thread| thread["id"] != first["id"])
        .unwrap()
        .clone();
    assert_eq!(
        std::path::PathBuf::from(second["workspace"].as_str().unwrap()),
        backend.directory.path().canonicalize().unwrap()
    );
    assert_eq!(
        backend.threads().await["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|thread| thread["id"] == first["id"])
            .unwrap()["workspace"],
        first["workspace"]
    );
    std::fs::write(&path, "[workspace]\nnew_cwd = \"follow\"\n").unwrap();
    edit(&tui, "workspace.new_cwd").await;
    tui.wait("follow").await;
    tui.send(b"\x1b");
    tui.wait_absent("Edit setting").await;
    close_settings(&tui).await;
    tui.prefix('1');
    tui.wait("Solmu 1 · idle").await;
    tui.prefix('v');
    tui.wait("Solmu 3 · idle").await;
    tui.wait("Ready").await;
    let threads = backend.threads().await;
    let third = threads["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|thread| thread["id"] != first["id"] && thread["id"] != second["id"])
        .unwrap();
    assert_eq!(third["workspace"], first["workspace"]);
    tui.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_configuration_output_is_complete_and_loads_as_a_valid_configuration() {
    let backend = Backend::start().await;
    let output = std::process::Command::new(binary("muxer"))
        .arg("--default-config")
        .output()
        .unwrap();
    assert!(output.status.success());
    let source = String::from_utf8(output.stdout).unwrap();
    for key in [
        "prefix",
        "new_tab",
        "select_tab_8",
        "settings",
        "toggle_sidebar",
        "theme",
        "workspace",
        "headless_columns",
    ] {
        assert!(source.contains(key), "Missing {key}");
    }
    let path = backend.directory.path().join("generated.toml");
    std::fs::write(&path, format!("\u{feff}{source}")).unwrap();
    let mut tui = Terminal::start_config(&backend, &path);
    tui.wait("Ready").await;
    assert!(!tui.contents().contains("Configuration error"));
    tui.prefix(',');
    tui.wait("Automatic updates").await;
    tui.wait("SOLMU    New conversation").await;
    solmu_e2e::support::capture_terminal(tui.screen.lock().unwrap().screen(), "muxer-settings");
    tui.send(b"keys.prefix");
    tui.wait("> keys.prefix").await;
    tui.wait("keys.prefix    ctrl+b").await;
    close_settings(&tui).await;
    tui.resize(18, 60);
    tui.wait_resized(60).await;
    tui.click("solmu / muxer").await;
    tui.wait("Automatic updates").await;
    close_settings(&tui).await;
    tui.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn home_policy_starts_conversations_in_the_users_home_and_keeps_that_policy_for_new_tabs() {
    let backend = Backend::start().await;
    let home = backend.directory.path().join("Home folder");
    std::fs::create_dir(&home).unwrap();
    std::fs::write(config_path(&backend), "[workspace]\nnew_cwd = \"home\"\n").unwrap();
    let mut tui = Terminal::start_home(&backend, &home);
    tui.wait("Ready").await;
    tui.prefix('n');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready").await;
    let threads = backend.threads().await;
    assert_eq!(threads["items"].as_array().unwrap().len(), 2);
    for thread in threads["items"].as_array().unwrap() {
        assert_eq!(
            std::path::PathBuf::from(thread["workspace"].as_str().unwrap()),
            home.canonicalize().unwrap()
        );
    }
    tui.exit().await;
}
