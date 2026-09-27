use super::*;

#[tokio::test]
async fn backend_serves_published_pages_and_assets_without_rewriting_api_errors() {
    let backend = Backend::start().await;
    let directory = backend.directory.path().join("web");
    std::fs::create_dir_all(directory.join("assets")).unwrap();
    let index = "<!doctype html><title>Solmu</title><div id=\"root\"></div>";
    std::fs::write(directory.join("index.html"), index).unwrap();
    std::fs::write(directory.join("assets/app.js"), "console.log('Solmu')").unwrap();
    std::fs::write(backend.directory.path().join("private.txt"), "Private data").unwrap();
    for path in ["/", "/profile", "/threads/saved-thread"] {
        let response = backend
            .client
            .get(backend.endpoint(path))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert!(
            response.headers()["content-type"]
                .to_str()
                .unwrap()
                .contains("text/html")
        );
        assert_eq!(response.text().await.unwrap(), index);
    }
    let asset = backend
        .client
        .get(backend.endpoint("/assets/app.js"))
        .send()
        .await
        .unwrap();
    assert_eq!(asset.status(), StatusCode::OK);
    assert!(
        asset.headers()["content-type"]
            .to_str()
            .unwrap()
            .contains("javascript")
    );
    assert_eq!(asset.text().await.unwrap(), "console.log('Solmu')");
    assert_eq!(
        backend
            .client
            .head(backend.endpoint("/"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        backend
            .client
            .post(backend.endpoint("/"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    for path in [
        "/assets/missing.js",
        "/assets/%2e%2e%2fprivate.txt",
        "/private.txt",
        "/.env",
    ] {
        assert_eq!(
            backend
                .client
                .get(backend.endpoint(path))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    let error = backend
        .client
        .get(backend.endpoint("/api/v1/missing"))
        .send()
        .await
        .unwrap();
    assert_eq!(error.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        error.json::<Value>().await.unwrap()["error"]["code"],
        "not_found"
    );
    assert!(
        backend.threads().await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    std::fs::write(directory.join("index.html"), "Updated Solmu").unwrap();
    assert_eq!(
        backend
            .client
            .get(backend.endpoint("/"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "Updated Solmu"
    );
}

#[tokio::test]
async fn missing_web_files_do_not_prevent_the_backend_api_from_running() {
    let backend = Backend::start().await;
    assert_eq!(
        backend
            .client
            .get(backend.endpoint("/"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert!(
        backend.threads().await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
