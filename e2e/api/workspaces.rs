use super::*;

#[tokio::test]
async fn thread_workspace_defaults_and_explicit_directories_are_persistent() {
    let mut backend = Backend::start().await;
    let thread = backend.create_thread("Default workspace").await;
    let default = backend
        .directory
        .path()
        .join("workspace")
        .canonicalize()
        .unwrap();
    assert_eq!(
        std::path::PathBuf::from(thread["workspace"].as_str().unwrap()),
        default
    );
    let workspace = backend.directory.path().join("project with spaces");
    std::fs::create_dir(&workspace).unwrap();
    let thread: Value = backend
        .client
        .post(backend.endpoint("/api/v1/threads"))
        .json(&json!({"workspace":workspace}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = thread["id"].as_str().unwrap().to_owned();
    assert_eq!(
        std::path::PathBuf::from(thread["workspace"].as_str().unwrap()),
        workspace.canonicalize().unwrap()
    );
    for path in ["relative", "missing-directory"] {
        assert_eq!(
            backend
                .client
                .post(backend.endpoint("/api/v1/threads"))
                .json(&json!({"workspace":path}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    backend.restart().await;
    let stored: Value = backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{id}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(stored["workspace"], thread["workspace"]);
}
