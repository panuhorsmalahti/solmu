use crate::{
    api::state::AppState,
    storage::{
        messages,
        scheduled_tasks::{self, ScheduledTask, TaskRun},
    },
};
use chrono::Utc;
use std::time::Duration;

pub async fn run(state: AppState) {
    loop {
        match scheduled_tasks::due(&state.pool, Utc::now()).await {
            Ok(due) => {
                for (task, run) in due {
                    state.tasks_changed();
                    let state = state.clone();
                    tokio::spawn(async move { execute(state, task, run).await });
                }
            }
            Err(error) => eprintln!("Scheduled task poll failed: {error:?}"),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn execute(state: AppState, task: ScheduledTask, run: TaskRun) {
    let mut message_id = None;
    let result = async {
        let message = messages::create(&state.pool, &task.thread_id, "user", &task.prompt, None)
            .await
            .map_err(|error| format!("Cannot save scheduled prompt: {error:?}"))?;
        message_id = Some(message.id.clone());
        state.changed(&task.thread_id);
        tokio::time::timeout(
            Duration::from_secs(1800),
            crate::api::responses::run_scheduled(state.clone(), task.thread_id.clone(), message.id),
        )
        .await
        .map_err(|_| "Scheduled agent turn timed out after 30 minutes".to_owned())?
    }
    .await;
    let (status, error) = match result {
        Ok(_) => ("completed", None),
        Err(error) => ("failed", Some(error)),
    };
    if let Err(error) = scheduled_tasks::finish(
        &state.pool,
        &run.id,
        &task.id,
        status,
        error.as_deref(),
        message_id.as_deref(),
    )
    .await
    {
        eprintln!("Cannot finish scheduled task {}: {error:?}", task.id);
    }
    state.tasks_changed();
    state.changed(&task.thread_id);
}
