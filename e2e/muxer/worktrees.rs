use super::{sessions::Session, *};
use serde_json::Value;
use std::{
    path::PathBuf,
    process::{Command, Output},
};

fn git(directory: &std::path::Path, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .unwrap()
}

fn command(session: &Session<'_>, args: &[&str]) -> Value {
    let output = session.command(args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice::<Value>(&output.stdout).unwrap()["result"].clone()
}

fn repository(directory: &std::path::Path) {
    std::fs::create_dir_all(directory).unwrap();
    let init = Command::new("git")
        .args(["init", "-q"])
        .arg(directory)
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    std::fs::write(directory.join("README.md"), "worktree fixture\n").unwrap();
    assert!(git(directory, &["add", "README.md"]).status.success());
    let commit = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args([
            "-c",
            "user.name=Solmu E2E",
            "-c",
            "user.email=e2e@example.invalid",
            "commit",
            "-qm",
            "initial",
        ])
        .output()
        .unwrap();
    assert!(
        commit.status.success(),
        "{}",
        String::from_utf8_lossy(&commit.stderr)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worktree_create_handles_new_and_existing_branches_and_restores_metadata() {
    let backend = Backend::start().await;
    let repo = backend.directory.path().join("project");
    repository(&repo);
    assert!(git(&repo, &["branch", "existing-branch"]).status.success());
    let session = Session {
        backend: &backend,
        name: "worktrees",
    };
    assert!(session.command(&["server", "start"]).status.success());

    let source = command(
        &session,
        &[
            "space",
            "create",
            "--cwd",
            repo.to_str().unwrap(),
            "--name",
            "Project",
        ],
    );
    let source_id = source["space"]["id"].as_u64().unwrap().to_string();
    let new_path = backend.directory.path().join("checkouts/new-branch");
    let created = command(
        &session,
        &[
            "worktree",
            "create",
            "--space",
            &source_id,
            "--branch",
            "feature/worktree",
            "--base",
            "HEAD",
            "--path",
            new_path.to_str().unwrap(),
            "--focus",
        ],
    );
    assert_eq!(created["worktree"]["branch"], "feature/worktree");
    let created_path = PathBuf::from(created["worktree"]["path"].as_str().unwrap());
    assert_eq!(
        created_path.canonicalize().unwrap(),
        new_path.canonicalize().unwrap()
    );
    assert!(new_path.join("README.md").is_file());
    assert!(
        git(&new_path, &["branch", "--show-current"])
            .status
            .success()
    );
    assert_eq!(
        String::from_utf8_lossy(&git(&new_path, &["branch", "--show-current"]).stdout).trim(),
        "feature/worktree"
    );
    let worktree_id = created["created"]["space"]["id"]
        .as_u64()
        .unwrap()
        .to_string();
    let child = &created["created"]["space"]["worktree"];
    assert_eq!(child["branch"], "feature/worktree");
    assert_eq!(child["primary"], false);
    let parent = command(&session, &["space", "get", &source_id]);
    assert_eq!(parent["worktree"]["primary"], true);
    assert_eq!(
        parent["worktree"]["repo_root"],
        serde_json::to_value(repo.canonicalize().unwrap()).unwrap()
    );

    let existing_path = backend.directory.path().join("checkouts/existing-branch");
    let existing = command(
        &session,
        &[
            "worktree",
            "create",
            "--space",
            &source_id,
            "--branch",
            "existing-branch",
            "--path",
            existing_path.to_str().unwrap(),
        ],
    );
    assert_eq!(existing["worktree"]["branch"], "existing-branch");
    assert_eq!(existing["worktree"]["base"], Value::Null);
    assert!(
        git(&existing_path, &["branch", "--show-current"])
            .status
            .success()
    );

    let listed = command(&session, &["worktree", "list", "--space", &source_id]);
    let items = listed["worktrees"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["primary"], true);
    assert!(
        items.iter().any(|item| {
            item["branch"] == "feature/worktree"
                && item["open_space"] == worktree_id.parse::<u64>().unwrap()
        }),
        "{listed}"
    );
    assert!(items.iter().any(|item| item["branch"] == "existing-branch"));

    let invalid = session.command(&[
        "worktree",
        "create",
        "--space",
        &source_id,
        "--branch",
        "../escape",
        "--path",
        backend.directory.path().join("invalid").to_str().unwrap(),
    ]);
    assert!(!invalid.status.success());
    assert!(!backend.directory.path().join("invalid").exists());

    assert!(session.command(&["server", "stop"]).status.success());
    assert!(session.command(&["server", "start"]).status.success());
    let restored: Value = command(&session, &["space", "get", &worktree_id]);
    assert_eq!(restored["worktree"]["branch"], "feature/worktree");
    assert_eq!(restored["worktree"]["primary"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worktree_branch_prompt_creates_a_space_in_the_terminal_ui() {
    let backend = Backend::start().await;
    let repo = backend.directory.path().join("ui-project");
    repository(&repo);
    let config = backend.directory.path().join("muxer-state/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(
        config,
        format!(
            "[worktrees]\ndirectory = {:?}\n",
            backend.directory.path().join("checkouts").to_string_lossy()
        ),
    )
    .unwrap();

    let mut tui = Terminal::start(&backend, &[&repo]);
    tui.wait("Ready").await;
    tui.click("+ Worktree").await;
    tui.wait("New Git worktree").await;
    solmu_e2e::support::capture_terminal(tui.screen.lock().unwrap().screen(), "muxer-worktrees");
    tui.command("feature/ui-worktree");
    tui.wait("ui-project · feature/ui-worktree").await;
    assert!(
        backend
            .directory
            .path()
            .join("checkouts/ui-project/feature-ui-worktree/README.md")
            .is_file()
    );
    tui.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worktree_open_reuses_existing_spaces_and_finds_paths_or_branches() {
    let backend = Backend::start().await;
    let repo = backend.directory.path().join("open-project");
    repository(&repo);
    assert!(git(&repo, &["branch", "review-branch"]).status.success());
    let checkout = backend.directory.path().join("external-checkout");
    assert!(
        git(
            &repo,
            &[
                "worktree",
                "add",
                checkout.to_str().unwrap(),
                "review-branch"
            ]
        )
        .status
        .success()
    );
    let session = Session {
        backend: &backend,
        name: "worktree-open",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let source = command(
        &session,
        &[
            "space",
            "create",
            "--cwd",
            repo.to_str().unwrap(),
            "--name",
            "Project",
        ],
    );
    let source_id = source["space"]["id"].as_u64().unwrap().to_string();

    let opened = command(
        &session,
        &[
            "worktree",
            "open",
            "--space",
            &source_id,
            "--branch",
            "review-branch",
        ],
    );
    assert_eq!(opened["opened"], true);
    let space_id = opened["space"]["id"].as_u64().unwrap().to_string();
    assert_eq!(opened["space"]["worktree"]["branch"], "review-branch");
    assert_eq!(opened["space"]["worktree"]["primary"], false);
    let reopened = command(
        &session,
        &[
            "worktree",
            "open",
            "--space",
            &source_id,
            "--path",
            checkout.to_str().unwrap(),
        ],
    );
    assert_eq!(reopened["opened"], false);
    assert_eq!(reopened["space"]["id"], space_id.parse::<u64>().unwrap());
    let nested_path = backend.directory.path().join("nested-checkout");
    let nested = command(
        &session,
        &[
            "worktree",
            "create",
            "--space",
            &space_id,
            "--branch",
            "nested-branch",
            "--path",
            nested_path.to_str().unwrap(),
        ],
    );
    assert_eq!(nested["created"]["space"]["worktree"]["primary"], false);
    let primary = command(&session, &["space", "get", &source_id]);
    assert_eq!(primary["worktree"]["primary"], true);
    assert!(session.command(&["server", "stop"]).status.success());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worktree_remove_protects_dirty_checkouts_and_keeps_the_branch() {
    let backend = Backend::start().await;
    let repo = backend.directory.path().join("remove-project");
    repository(&repo);
    let session = Session {
        backend: &backend,
        name: "worktree-remove",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let source = command(
        &session,
        &[
            "space",
            "create",
            "--cwd",
            repo.to_str().unwrap(),
            "--name",
            "Project",
        ],
    );
    let source_id = source["space"]["id"].as_u64().unwrap().to_string();
    let path = backend.directory.path().join("remove-checkout");
    let created = command(
        &session,
        &[
            "worktree",
            "create",
            "--space",
            &source_id,
            "--branch",
            "remove-branch",
            "--path",
            path.to_str().unwrap(),
        ],
    );
    let worktree_id = created["created"]["space"]["id"]
        .as_u64()
        .unwrap()
        .to_string();
    std::fs::write(path.join("uncommitted.txt"), "keep unless forced\n").unwrap();

    let rejected = session.command(&["worktree", "remove", "--space", &worktree_id]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("uncommitted changes"));
    assert!(path.exists());
    assert_eq!(
        command(&session, &["space", "get", &worktree_id])["id"],
        worktree_id.parse::<u64>().unwrap()
    );

    let removed = command(
        &session,
        &["worktree", "remove", "--space", &worktree_id, "--force"],
    );
    assert_eq!(removed["branch"], "remove-branch");
    assert!(!path.exists());
    assert!(
        git(
            &repo,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                "refs/heads/remove-branch"
            ]
        )
        .status
        .success()
    );
    let spaces = command(&session, &["space", "list"]);
    let spaces = spaces["items"].as_array().unwrap();
    assert!(
        spaces
            .iter()
            .any(|space| space["id"] == source_id.parse::<u64>().unwrap())
    );
    assert!(
        spaces
            .iter()
            .all(|space| space["id"] != worktree_id.parse::<u64>().unwrap())
    );
    assert!(session.command(&["server", "stop"]).status.success());
}
