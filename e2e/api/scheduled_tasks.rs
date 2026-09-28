use super::*;
use chrono::{Duration as ChronoDuration, Utc};

async fn tasks(backend: &Backend) -> Value {
    backend
        .client
        .get(backend.endpoint("/api/v1/tasks"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn one_shot_task_runs_agent_tools_once_and_keeps_run_history() {
    let mut backend = Backend::start().await;
    let at = (Utc::now() + ChronoDuration::seconds(3)).to_rfc3339();
    let response = backend.client.post(backend.endpoint("/api/v1/tasks"))
        .json(&json!({"name":"Workspace check","prompt":"TOOLS","schedule_kind":"once","schedule":at}))
        .send().await.unwrap().error_for_status().unwrap();
    let task: Value = response.json().await.unwrap();
    let id = task["id"].as_str().unwrap();
    assert_eq!(task["enabled"], true);
    assert_eq!(task["schedule_kind"], "once");
    assert_eq!(tasks(&backend).await["items"].as_array().unwrap().len(), 1);
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let task: Value = backend
                .client
                .get(backend.endpoint(&format!("/api/v1/tasks/{id}")))
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap()
                .json()
                .await
                .unwrap();
            if task["last_status"] == "completed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    let task = tasks(&backend).await["items"][0].clone();
    assert_eq!(task["enabled"], false);
    assert!(task["next_run_at"].is_null());
    let thread = task["thread_id"].as_str().unwrap();
    assert_eq!(
        backend.messages(thread).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let activity: Value = backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{thread}/tools")))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(activity["items"].as_array().unwrap().len(), 6);
    let runs: Value = backend
        .client
        .get(backend.endpoint(&format!("/api/v1/tasks/{id}/runs")))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(runs["items"].as_array().unwrap().len(), 1);
    assert_eq!(runs["items"][0]["status"], "completed");
    backend.restart().await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let after = tasks(&backend).await;
    assert_eq!(after["items"][0]["last_status"], "completed");
    let runs: Value = backend
        .client
        .get(backend.endpoint(&format!("/api/v1/tasks/{id}/runs")))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(runs["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn cron_task_can_run_now_pause_edit_resume_and_delete_without_erasing_conversation() {
    let backend = Backend::start().await;
    for schedule in ["bad", "* * *", "61 * * * *"] {
        let invalid = backend
            .client
            .post(backend.endpoint("/api/v1/tasks"))
            .json(
                &json!({"name":"Bad","prompt":"Hello","schedule_kind":"cron","schedule":schedule}),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    }
    let task: Value = backend.client.post(backend.endpoint("/api/v1/tasks"))
        .json(&json!({"name":"Daily note","prompt":"Hello","schedule_kind":"cron","schedule":"* * * * *"}))
        .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    let id = task["id"].as_str().unwrap();
    let thread = task["thread_id"].as_str().unwrap();
    assert!(task["next_run_at"].is_string());
    let paused: Value = backend
        .client
        .patch(backend.endpoint(&format!("/api/v1/tasks/{id}")))
        .json(&json!({"enabled":false,"name":"Hourly note","prompt":"Hello again"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(paused["enabled"], false);
    assert_eq!(paused["name"], "Hourly note");
    let invalid_paused = backend
        .client
        .patch(backend.endpoint(&format!("/api/v1/tasks/{id}")))
        .json(&json!({"schedule":"not a cron"}))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid_paused.status(), StatusCode::BAD_REQUEST);
    let resumed: Value = backend
        .client
        .patch(backend.endpoint(&format!("/api/v1/tasks/{id}")))
        .json(&json!({"enabled":true,"schedule":"0 * * * *"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(resumed["next_run_at"].is_string());
    let run: Value = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/tasks/{id}/run")))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(run["status"], "running");
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let task: Value = backend
                .client
                .get(backend.endpoint(&format!("/api/v1/tasks/{id}")))
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap()
                .json()
                .await
                .unwrap();
            if task["last_status"] == "completed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        backend.messages(thread).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        backend
            .client
            .delete(backend.endpoint(&format!("/api/v1/tasks/{id}")))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert!(
        tasks(&backend).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        backend.messages(thread).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn agent_can_create_a_scheduled_task_with_the_tasks_tool() {
    let backend = Backend::start().await;
    let thread = backend.create_thread("Plan work").await;
    let id = thread["id"].as_str().unwrap();
    let at = (Utc::now() + ChronoDuration::hours(1)).to_rfc3339();
    let calls = json!([{"name":"Tasks","arguments":{
        "action":"create","name":"Follow up","prompt":"Check the workspace",
        "schedule_kind":"once","schedule":at
    }}]);
    let message = backend.send_message(id, &format!("TOOLS {calls}")).await;
    assert!(
        complete_reply(&backend, id, &message)
            .await
            .contains(&"done".to_owned())
    );
    let listing = tasks(&backend).await;
    assert_eq!(listing["items"].as_array().unwrap().len(), 1);
    assert_eq!(listing["items"][0]["name"], "Follow up");
    let activity: Value = backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{id}/tools")))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(activity["items"][0]["name"], "Tasks");
    assert_eq!(activity["items"][0]["status"], "completed");
}
