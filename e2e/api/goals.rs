use super::*;

#[tokio::test]
async fn goals_are_persistent_and_have_validatable_lifecycle_states() {
    let mut backend = Backend::start().await;
    let invalid = backend
        .client
        .post(backend.endpoint("/api/v1/goals"))
        .json(&json!({"objective":"  "}))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    let created: Value = backend
        .client
        .post(backend.endpoint("/api/v1/goals"))
        .json(&json!({"objective":"Ship the first version"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(created["status"], "active");
    let id = created["id"].as_str().unwrap();
    let list: Value = backend
        .client
        .get(backend.endpoint("/api/v1/goals"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list["items"][0]["id"], id);
    let paused: Value = backend
        .client
        .patch(backend.endpoint(&format!("/api/v1/goals/{id}")))
        .json(&json!({"status":"paused"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(paused["status"], "paused");
    backend.restart().await;
    let list: Value = backend
        .client
        .get(backend.endpoint("/api/v1/goals"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list["items"][0]["status"], "paused");
}

#[tokio::test]
async fn model_can_start_a_goal_with_the_registered_tool() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Goal request").await;
    let id = thread["id"].as_str().unwrap();
    let calls = json!([{"name":"Goals","arguments":{"action":"start","objective":"Prepare a release plan"}}]);
    let message = backend.send_message(id, &format!("TOOLS {calls}")).await;
    assert!(
        complete_reply(&backend, id, &message)
            .await
            .contains(&"done".to_owned())
    );
    let listing: Value = backend
        .client
        .get(backend.endpoint("/api/v1/goals"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listing["items"][0]["objective"], "Prepare a release plan");
}
