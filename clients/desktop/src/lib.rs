use futures_util::StreamExt;
use iced::{
    Element, Length, Task, Theme,
    widget::{button, column, container, row, scrollable, text, text_editor, text_input, tooltip},
};
use solmu_client::{
    Action, Api, AuditPage, AuditRun, CacheSummary, Connection, ModelCatalog, Profile,
    ScheduledTask, Session, TaskRun, Update,
};

mod appearance;
pub use appearance::theme;

pub struct Desktop {
    pub session: Session,
    draft: String,
    title: String,
    connected: bool,
    pending_refresh: bool,
    active: Option<iced::task::Handle>,
    profile_open: bool,
    profile: text_editor::Content,
    profile_original: String,
    profile_model: String,
    profile_original_model: String,
    profile_backend_default: Option<String>,
    profile_edited_at: String,
    settings_busy: bool,
    settings_notice: String,
    model_open: bool,
    skills_open: bool,
    mcp_open: bool,
    plugins_open: bool,
    audit_open: bool,
    audit_items: Vec<AuditRun>,
    audit_cache: Option<CacheSummary>,
    audit_next: Option<i64>,
    audit_starts: Vec<Option<i64>>,
    audit_page: usize,
    audit_expanded: Option<String>,
    audit_busy: bool,
    audit_dirty: bool,
    audit_error: String,
    tasks_open: bool,
    task_items: Vec<ScheduledTask>,
    task_runs: Vec<TaskRun>,
    task_selected: Option<String>,
    task_name: String,
    task_prompt: String,
    task_schedule: String,
    task_kind: String,
    task_editing: Option<String>,
    task_busy: bool,
    task_save_pending: bool,
    task_error: String,
    catalog: Option<ModelCatalog>,
    custom_model: String,
}

#[derive(Debug, Clone)]
pub enum Event {
    Action(Action),
    Updated(Update),
    Draft(String),
    Title(String),
    Send,
    Stop,
    StopFinished(Result<(), String>),
    Connection(Connection),
    OpenProfile,
    CloseSettings,
    ProfileEdited(text_editor::Action),
    ProfileLoaded(Result<Profile, String>),
    SaveProfile,
    ProfileSaved(Result<Profile, String>),
    ReloadProfile,
    OpenModels,
    OpenSkills,
    OpenMcp,
    OpenPlugins,
    OpenAudit,
    AuditLoaded(Result<AuditPage, String>),
    AuditNext,
    AuditPrevious,
    CloseAudit,
    AuditToggle(String),
    OpenTasks,
    TasksLoaded(Result<Vec<ScheduledTask>, String>),
    TaskRunsLoaded(Result<Vec<TaskRun>, String>),
    TaskMutated(Result<(), String>),
    TaskName(String),
    TaskPrompt(String),
    TaskSchedule(String),
    TaskKind(String),
    TaskSave,
    TaskEdit(String),
    TaskCancelEdit,
    TaskSelect(String),
    TaskRun(String),
    TaskToggle(String, bool),
    TaskDelete(String),
    ModelsLoaded(Result<ModelCatalog, String>),
    CustomModel(String),
    ProfileModel(String),
}

pub fn application(
    api: Api,
) -> iced::Application<impl iced::Program<State = Desktop, Message = Event, Theme = Theme>> {
    iced::application(
        move || Desktop::new(api.clone()),
        Desktop::update,
        Desktop::view,
    )
    .title("Solmu")
    .theme(theme())
    .subscription(|state| {
        iced::Subscription::run_with(state.session.api.base(), |base| {
            Api::new(base).changes().map(Event::Connection)
        })
    })
    .window_size((1120.0, 760.0))
}

impl Desktop {
    pub fn new(api: Api) -> (Self, Task<Event>) {
        let mut desktop = Self {
            session: Session::new(api),
            draft: String::new(),
            title: String::new(),
            connected: false,
            pending_refresh: false,
            active: None,
            profile_open: false,
            profile: text_editor::Content::new(),
            profile_original: String::new(),
            profile_model: String::new(),
            profile_original_model: String::new(),
            profile_backend_default: None,
            profile_edited_at: String::new(),
            settings_busy: false,
            settings_notice: String::new(),
            model_open: false,
            skills_open: false,
            mcp_open: false,
            plugins_open: false,
            audit_open: false,
            audit_items: Vec::new(),
            audit_cache: None,
            audit_next: None,
            audit_starts: vec![None],
            audit_page: 0,
            audit_expanded: None,
            audit_busy: false,
            audit_dirty: false,
            audit_error: String::new(),
            tasks_open: false,
            task_items: Vec::new(),
            task_runs: Vec::new(),
            task_selected: None,
            task_name: String::new(),
            task_prompt: String::new(),
            task_schedule: String::new(),
            task_kind: "once".into(),
            task_editing: None,
            task_busy: false,
            task_save_pending: false,
            task_error: String::new(),
            catalog: None,
            custom_model: String::new(),
        };
        let task = desktop.act(Action::New("New conversation".into()));
        (desktop, task)
    }

    fn act(&mut self, action: Action) -> Task<Event> {
        let updates = if matches!(action, Action::Refresh) {
            self.session.refresh()
        } else {
            self.session.begin(action)
        };
        if let Some(updates) = updates {
            let (task, handle) = Task::run(updates, Event::Updated).abortable();
            self.active = Some(handle.abort_on_drop());
            task
        } else {
            Task::none()
        }
    }

    fn load_audit(&mut self) -> Task<Event> {
        self.audit_busy = true;
        let api = self.session.api.clone();
        let before = self.audit_starts[self.audit_page];
        Task::perform(
            async move { api.audit(before, 20).await },
            Event::AuditLoaded,
        )
    }

    fn load_tasks(&self) -> Task<Event> {
        let api = self.session.api.clone();
        Task::perform(async move { api.tasks().await }, Event::TasksLoaded)
    }

    fn load_task_runs(&self, id: String) -> Task<Event> {
        let api = self.session.api.clone();
        Task::perform(
            async move { api.task_runs(&id).await },
            Event::TaskRunsLoaded,
        )
    }

    pub fn update(&mut self, event: Event) -> Task<Event> {
        let saved_profile = matches!(&event, Event::ProfileSaved(_));
        let opened_profile = matches!(&event, Event::OpenProfile);
        if matches!(
            &event,
            Event::OpenProfile
                | Event::ReloadProfile
                | Event::OpenSkills
                | Event::CloseSettings
                | Event::OpenModels
        ) || matches!(&event, Event::Action(action) if !matches!(action, Action::Refresh))
        {
            self.mcp_open = false;
            self.plugins_open = false;
            self.audit_open = false;
            self.tasks_open = false;
        }
        match event {
            Event::OpenTasks => {
                self.tasks_open = true;
                self.audit_open = false;
                self.profile_open = false;
                self.model_open = false;
                self.skills_open = false;
                self.mcp_open = false;
                self.plugins_open = false;
                self.load_tasks()
            }
            Event::TasksLoaded(result) => {
                match result {
                    Ok(items) => {
                        self.task_items = items;
                        self.task_error.clear();
                    }
                    Err(error) => self.task_error = error,
                }
                Task::none()
            }
            Event::TaskRunsLoaded(result) => {
                match result {
                    Ok(runs) => self.task_runs = runs,
                    Err(error) => self.task_error = error,
                }
                Task::none()
            }
            Event::TaskMutated(result) => {
                self.task_busy = false;
                match result {
                    Ok(()) => {
                        if self.task_save_pending {
                            self.task_editing = None;
                            self.task_name.clear();
                            self.task_prompt.clear();
                            self.task_schedule.clear();
                        }
                        self.task_save_pending = false;
                        self.task_error.clear();
                        let mut tasks = vec![self.load_tasks()];
                        if let Some(id) = self.task_selected.clone() {
                            tasks.push(self.load_task_runs(id));
                        }
                        Task::batch(tasks)
                    }
                    Err(error) => {
                        self.task_save_pending = false;
                        self.task_error = error;
                        Task::none()
                    }
                }
            }
            Event::TaskName(value) => {
                self.task_name = value;
                Task::none()
            }
            Event::TaskPrompt(value) => {
                self.task_prompt = value;
                Task::none()
            }
            Event::TaskSchedule(value) => {
                self.task_schedule = value;
                Task::none()
            }
            Event::TaskKind(value) => {
                self.task_kind = value;
                Task::none()
            }
            Event::TaskCancelEdit => {
                self.task_editing = None;
                self.task_name.clear();
                self.task_prompt.clear();
                self.task_schedule.clear();
                Task::none()
            }
            Event::TaskEdit(id) => {
                if let Some(task) = self.task_items.iter().find(|task| task.id == id) {
                    self.task_name = task.name.clone();
                    self.task_prompt = task.prompt.clone();
                    self.task_schedule = task.schedule.clone();
                    self.task_kind = task.schedule_kind.clone();
                    self.task_editing = Some(id);
                }
                Task::none()
            }
            Event::TaskSelect(id) => {
                if self.task_selected.as_deref() == Some(&id) {
                    self.task_selected = None;
                    self.task_runs.clear();
                    Task::none()
                } else {
                    self.task_selected = Some(id.clone());
                    self.task_runs.clear();
                    self.load_task_runs(id)
                }
            }
            Event::TaskSave => {
                if self.task_busy {
                    return Task::none();
                }
                self.task_busy = true;
                self.task_save_pending = true;
                let api = self.session.api.clone();
                let name = self.task_name.clone();
                let prompt = self.task_prompt.clone();
                let schedule = self.task_schedule.clone();
                let kind = self.task_kind.clone();
                let editing = self.task_editing.clone();
                Task::perform(
                    async move {
                        if let Some(id) = editing {
                            api.update_task(&id, serde_json::json!({"name":name,"prompt":prompt,"schedule_kind":kind,"schedule":schedule})).await.map(|_| ())
                        } else {
                            api.create_task(&name, &prompt, &kind, &schedule)
                                .await
                                .map(|_| ())
                        }
                    },
                    Event::TaskMutated,
                )
            }
            Event::TaskRun(id) => {
                if self.task_busy {
                    return Task::none();
                }
                self.task_busy = true;
                let api = self.session.api.clone();
                Task::perform(
                    async move { api.run_task(&id).await.map(|_| ()) },
                    Event::TaskMutated,
                )
            }
            Event::TaskToggle(id, enabled) => {
                if self.task_busy {
                    return Task::none();
                }
                self.task_busy = true;
                let api = self.session.api.clone();
                Task::perform(
                    async move {
                        api.update_task(&id, serde_json::json!({"enabled":enabled}))
                            .await
                            .map(|_| ())
                    },
                    Event::TaskMutated,
                )
            }
            Event::TaskDelete(id) => {
                if self.task_busy {
                    return Task::none();
                }
                self.task_busy = true;
                let api = self.session.api.clone();
                if self.task_selected.as_deref() == Some(&id) {
                    self.task_selected = None;
                    self.task_runs.clear();
                }
                Task::perform(
                    async move { api.delete_task(&id).await },
                    Event::TaskMutated,
                )
            }
            Event::OpenAudit => {
                self.audit_open = true;
                self.profile_open = false;
                self.skills_open = false;
                self.mcp_open = false;
                self.plugins_open = false;
                self.model_open = false;
                self.audit_page = 0;
                self.audit_starts = vec![None];
                self.audit_expanded = None;
                self.load_audit()
            }
            Event::CloseAudit => {
                self.audit_open = false;
                self.tasks_open = false;
                Task::none()
            }
            Event::AuditLoaded(result) => {
                self.audit_busy = false;
                match result {
                    Ok(page) => {
                        self.audit_items = page.items;
                        self.audit_next = page.next_cursor;
                        self.audit_cache = Some(page.cache_24h);
                        self.audit_error.clear();
                    }
                    Err(error) => self.audit_error = error,
                }
                if self.audit_open && std::mem::take(&mut self.audit_dirty) {
                    self.load_audit()
                } else {
                    Task::none()
                }
            }
            Event::AuditNext => {
                if self.audit_busy {
                    return Task::none();
                }
                let Some(cursor) = self.audit_next else {
                    return Task::none();
                };
                self.audit_page += 1;
                self.audit_starts.truncate(self.audit_page);
                self.audit_starts.push(Some(cursor));
                self.audit_expanded = None;
                self.load_audit()
            }
            Event::AuditPrevious => {
                if self.audit_busy || self.audit_page == 0 {
                    return Task::none();
                }
                self.audit_page -= 1;
                self.audit_expanded = None;
                self.load_audit()
            }
            Event::AuditToggle(id) => {
                if self.audit_expanded.as_deref() == Some(&id) {
                    self.audit_expanded = None;
                } else {
                    self.audit_expanded = Some(id);
                }
                Task::none()
            }
            Event::OpenPlugins => {
                self.plugins_open = true;
                self.mcp_open = false;
                self.skills_open = false;
                self.profile_open = false;
                self.model_open = false;
                Task::none()
            }
            Event::OpenMcp => {
                self.mcp_open = true;
                self.plugins_open = false;
                self.skills_open = false;
                self.profile_open = false;
                self.model_open = false;
                Task::none()
            }
            Event::OpenSkills => {
                self.skills_open = true;
                self.plugins_open = false;
                self.profile_open = false;
                self.model_open = false;
                Task::none()
            }
            Event::OpenProfile | Event::ReloadProfile => {
                self.skills_open = false;
                self.profile_open = true;
                self.model_open = false;
                self.settings_busy = true;
                if opened_profile {
                    self.settings_notice.clear();
                }
                let api = self.session.api.clone();
                Task::perform(async move { api.profile().await }, Event::ProfileLoaded)
            }
            Event::ProfileLoaded(result) | Event::ProfileSaved(result) => {
                self.settings_busy = false;
                match result {
                    Ok(profile) => {
                        let unchanged = self.profile_original == profile.system_prompt
                            && self.profile_original_model
                                == profile.model.as_deref().unwrap_or_default();
                        let keep_saved = !saved_profile
                            && unchanged
                            && self.settings_notice.starts_with("Profile saved");
                        self.profile_original = profile.system_prompt;
                        self.profile = text_editor::Content::with_text(&self.profile_original);
                        self.profile_model = profile.model.unwrap_or_default();
                        self.profile_original_model = self.profile_model.clone();
                        self.profile_backend_default = profile.backend_default_model;
                        self.profile_edited_at = profile.edited_at;
                        if !keep_saved {
                            self.settings_notice = if saved_profile {
                                "Profile saved · changes apply to subsequent replies"
                            } else {
                                "Profile ready"
                            }
                            .into();
                        }
                    }
                    Err(error) => self.settings_notice = error,
                }
                Task::none()
            }
            Event::ProfileEdited(action) => {
                if !self.settings_busy {
                    self.profile.perform(action);
                    self.settings_notice.clear();
                }
                Task::none()
            }
            Event::SaveProfile => {
                if self.settings_busy || self.profile.text().trim().is_empty() {
                    return Task::none();
                }
                self.settings_busy = true;
                let api = self.session.api.clone();
                let prompt = self.profile.text();
                let model = (!self.profile_model.trim().is_empty())
                    .then(|| self.profile_model.trim().to_owned());
                Task::perform(
                    async move { api.save_profile(&prompt, model.as_deref()).await },
                    Event::ProfileSaved,
                )
            }
            Event::CloseSettings => {
                self.skills_open = false;
                self.profile_open = false;
                self.model_open = false;
                Task::none()
            }
            Event::OpenModels => {
                self.skills_open = false;
                self.model_open = true;
                self.profile_open = false;
                self.settings_busy = true;
                self.settings_notice.clear();
                self.catalog = None;
                self.custom_model = self
                    .session
                    .current
                    .as_ref()
                    .and_then(|thread| thread.model.clone())
                    .unwrap_or_default();
                let api = self.session.api.clone();
                Task::perform(async move { api.models().await }, Event::ModelsLoaded)
            }
            Event::ModelsLoaded(result) => {
                self.settings_busy = false;
                match result {
                    Ok(catalog) => self.catalog = Some(catalog),
                    Err(error) => self.settings_notice = error,
                }
                Task::none()
            }
            Event::CustomModel(value) => {
                self.custom_model = value;
                Task::none()
            }
            Event::ProfileModel(value) => {
                if !self.settings_busy {
                    self.profile_model = value;
                    self.settings_notice.clear();
                }
                Task::none()
            }
            Event::Stop => {
                if !self.session.responding {
                    return Task::none();
                }
                self.session.stopping();
                let api = self.session.api.clone();
                let id = self
                    .session
                    .current
                    .as_ref()
                    .expect("selected thread")
                    .id
                    .clone();
                Task::perform(async move { api.stop(&id).await }, Event::StopFinished)
            }
            Event::StopFinished(Ok(())) => Task::none(),
            Event::StopFinished(Err(error)) => {
                if let Some(handle) = self.active.take() {
                    handle.abort();
                }
                self.update(Event::Updated(Update::Failed(error)))
            }
            Event::Connection(change) => {
                match change {
                    Connection::ProfileChanged => {
                        if self.profile_open && !self.settings_busy {
                            if self.profile.text() == self.profile_original
                                && self.profile_model == self.profile_original_model
                            {
                                return self.update(Event::ReloadProfile);
                            }
                            self.settings_notice =
                                "Profile changed elsewhere. Your draft is unchanged.".into();
                        }
                    }
                    Connection::Disconnected => self.connected = false,
                    Connection::TasksChanged => {
                        if self.tasks_open {
                            let mut tasks = vec![self.load_tasks()];
                            if let Some(id) = self.task_selected.clone() {
                                tasks.push(self.load_task_runs(id));
                            }
                            return Task::batch(tasks);
                        }
                    }
                    Connection::Connected | Connection::Changed => {
                        self.connected = true;
                        if self.tasks_open {
                            let mut tasks = vec![self.load_tasks(), self.act(Action::Refresh)];
                            if let Some(id) = self.task_selected.clone() {
                                tasks.push(self.load_task_runs(id));
                            }
                            return Task::batch(tasks);
                        }
                        if self.audit_open {
                            if self.audit_busy {
                                self.audit_dirty = true;
                            } else {
                                return Task::batch([self.load_audit(), self.act(Action::Refresh)]);
                            }
                        }
                        if self.session.busy {
                            self.pending_refresh = true;
                        } else {
                            return self.act(Action::Refresh);
                        }
                    }
                }
                Task::none()
            }
            Event::Action(action) => {
                if !matches!(action, Action::Refresh) {
                    self.skills_open = false;
                    self.profile_open = false;
                    self.model_open = false;
                    self.audit_open = false;
                    self.tasks_open = false;
                }
                self.act(action)
            }
            Event::Updated(update) => {
                let opened = matches!(&update, Update::Opened(thread, _) if thread.as_ref().map(|thread| &thread.id) != self.session.current.as_ref().map(|thread| &thread.id));
                let old_title = self
                    .session
                    .current
                    .as_ref()
                    .map(|thread| thread.title.clone())
                    .unwrap_or_default();
                self.session.apply(update);
                if opened {
                    self.title = self
                        .session
                        .current
                        .as_ref()
                        .map(|thread| thread.title.clone())
                        .unwrap_or_default();
                    self.draft.clear();
                } else if self.title == old_title {
                    self.title = self
                        .session
                        .current
                        .as_ref()
                        .map(|thread| thread.title.clone())
                        .unwrap_or_default();
                }
                if !self.session.busy && self.pending_refresh {
                    self.pending_refresh = false;
                    self.act(Action::Refresh)
                } else {
                    Task::none()
                }
            }
            Event::Draft(value) => {
                self.draft = value;
                Task::none()
            }
            Event::Title(value) => {
                self.title = value;
                Task::none()
            }
            Event::Send => {
                if self.session.busy
                    || self.session.current.is_none()
                    || self.draft.trim().is_empty()
                {
                    return Task::none();
                }
                let content = std::mem::take(&mut self.draft);
                self.act(Action::Send(content))
            }
        }
    }

    pub fn view(&self) -> Element<'_, Event> {
        let enabled = !self.session.busy;
        let new = tooltip(
            button(text("+").size(27))
                .style(appearance::ghost)
                .on_press_maybe(
                    enabled.then(|| Event::Action(Action::New("New conversation".into()))),
                ),
            "New thread",
            tooltip::Position::Bottom,
        );
        let mut list = column![
            row![
                text("Conversations").size(13),
                iced::widget::space().width(Length::Fill),
                new
            ]
            .align_y(iced::Alignment::Center)
        ]
        .spacing(12);
        for thread in &self.session.threads {
            let selected = self
                .session
                .current
                .as_ref()
                .is_some_and(|current| current.id == thread.id);
            list = list.push(
                button(text(&thread.title).size(14))
                    .width(Length::Fill)
                    .padding(12)
                    .style(move |theme, status| appearance::thread(theme, status, selected))
                    .on_press_maybe(
                        enabled.then(|| Event::Action(Action::Open(thread.id.clone()))),
                    ),
            );
        }
        let sidebar = container(
            column![
                text("solmu").size(32),
                iced::widget::space().height(22),
                button("Profile")
                    .width(Length::Fill)
                    .padding(12)
                    .style(appearance::ghost)
                    .on_press_maybe(enabled.then_some(Event::OpenProfile)),
                button("Audit")
                    .width(Length::Fill)
                    .padding(12)
                    .style(appearance::ghost)
                    .on_press(Event::OpenAudit),
                button("Tasks")
                    .width(Length::Fill)
                    .padding(12)
                    .style(appearance::ghost)
                    .on_press(Event::OpenTasks),
                scrollable(list).height(Length::Fill),
                text("YOUR IDEAS, CONNECTED")
                    .size(10)
                    .color(appearance::MUTED)
            ]
            .spacing(8),
        )
        .padding(24)
        .width(270)
        .height(Length::Fill)
        .style(appearance::sidebar);

        if self.tasks_open {
            let mut task_controls = row![
                button(if self.task_editing.is_some() {
                    "Save task"
                } else {
                    "Create task"
                })
                .on_press_maybe(
                    (!self.task_busy
                        && !self.task_name.trim().is_empty()
                        && !self.task_prompt.trim().is_empty()
                        && !self.task_schedule.trim().is_empty())
                    .then_some(Event::TaskSave)
                )
            ]
            .spacing(12);
            if self.task_editing.is_some() {
                task_controls = task_controls.push(
                    button("Cancel edit")
                        .style(appearance::ghost)
                        .on_press(Event::TaskCancelEdit),
                );
            }
            let mut content = column![
                button("Back to conversation").style(appearance::ghost).on_press(Event::CloseAudit),
                text("Tasks").size(36),
                text("Run Solmu later or on a recurring UTC schedule. Each task keeps its own conversation.").size(14).color(appearance::MUTED),
                text(if self.task_editing.is_some() { "Edit task" } else { "New task" }).size(22),
                text_input("Name", &self.task_name).on_input(Event::TaskName).style(appearance::input),
                text_input("What should Solmu do?", &self.task_prompt).on_input(Event::TaskPrompt).style(appearance::input),
                row![
                    button("One time").style(appearance::ghost).on_press(Event::TaskKind("once".into())),
                    button("Cron").style(appearance::ghost).on_press(Event::TaskKind("cron".into())),
                    text(format!("Selected: {}", self.task_kind)).size(12).color(appearance::MUTED),
                ].spacing(12),
                text_input(if self.task_kind == "cron" { "0 9 * * * (UTC)" } else { "2026-10-01T09:00:00Z" }, &self.task_schedule).on_input(Event::TaskSchedule).style(appearance::input),
                task_controls,
                text(&self.task_error).size(13),
                text("Scheduled tasks").size(22),
            ].spacing(12);
            if self.task_items.is_empty() {
                content = content.push(text("No tasks yet.").size(14));
            }
            for task in &self.task_items {
                let id = task.id.clone();
                let mut card = column![
                    text(&task.name).size(20),
                    text(&task.prompt).size(14),
                    text(format!(
                        "{} · {} · {}",
                        task.schedule_kind,
                        task.schedule,
                        if task.running {
                            "Running"
                        } else if task.enabled {
                            "Scheduled"
                        } else if task.schedule_kind == "once"
                            && task.last_status.as_deref() == Some("completed")
                        {
                            "Completed"
                        } else {
                            "Paused"
                        }
                    ))
                    .size(12)
                    .color(appearance::MUTED),
                    text(format!(
                        "Next: {} · Last: {}",
                        task.next_run_at.as_deref().unwrap_or("—"),
                        task.last_status.as_deref().unwrap_or("Never")
                    ))
                    .size(12)
                    .color(appearance::MUTED),
                    row![
                        button("Conversation")
                            .style(appearance::ghost)
                            .on_press(Event::Action(Action::Open(task.thread_id.clone()))),
                        button("Run now").style(appearance::ghost).on_press_maybe(
                            (!self.task_busy && !task.running)
                                .then_some(Event::TaskRun(id.clone()))
                        ),
                        button(if task.enabled {
                            "Pause"
                        } else if task.schedule_kind == "once"
                            && task.last_status.as_deref() == Some("completed")
                        {
                            "Completed"
                        } else {
                            "Resume"
                        })
                        .style(appearance::ghost)
                        .on_press_maybe(
                            (!self.task_busy
                                && !task.running
                                && !(task.schedule_kind == "once"
                                    && task.last_status.as_deref() == Some("completed")))
                            .then_some(Event::TaskToggle(id.clone(), !task.enabled))
                        ),
                        button("Edit")
                            .style(appearance::ghost)
                            .on_press(Event::TaskEdit(id.clone())),
                        button("Delete").style(appearance::danger).on_press_maybe(
                            (!self.task_busy && !task.running)
                                .then_some(Event::TaskDelete(id.clone()))
                        ),
                        button(if self.task_selected.as_deref() == Some(&id) {
                            "Hide runs"
                        } else {
                            "Run history"
                        })
                        .style(appearance::ghost)
                        .on_press(Event::TaskSelect(id.clone())),
                    ]
                    .spacing(8),
                ]
                .spacing(8);
                if self.task_selected.as_deref() == Some(&id) {
                    if self.task_runs.is_empty() {
                        card = card.push(text("No runs yet.").size(12));
                    }
                    for run in &self.task_runs {
                        card = card.push(
                            text(format!(
                                "{} · {}{}",
                                run.status,
                                run.started_at,
                                run.error
                                    .as_ref()
                                    .map(|e| format!(" · {e}"))
                                    .unwrap_or_default()
                            ))
                            .size(12),
                        );
                    }
                }
                content = content.push(
                    container(card)
                        .padding(18)
                        .width(Length::Fill)
                        .style(|theme| appearance::message(theme, false)),
                );
            }
            return row![
                sidebar,
                container(scrollable(content).height(Length::Fill))
                    .padding(40)
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .height(Length::Fill)
            .into();
        }

        if self.audit_open {
            let mut entries = column![
                button("Back to conversation")
                    .style(appearance::ghost)
                    .on_press(Event::CloseAudit),
                text("Audit").size(36),
                text("Tool calls across every conversation · newest first")
                    .size(14)
                    .color(appearance::MUTED),
            ]
            .spacing(14);
            if let Some(cache) = &self.audit_cache {
                let rate = cache
                    .hit_rate_percent
                    .map_or("—".into(), |rate| format!("{rate:.1}%"));
                entries = entries.push(
                    container(
                        column![
                            text("PROMPT CACHE · LAST 24 HOURS")
                                .size(11)
                                .color(appearance::MUTED),
                            text(rate).size(36).color(appearance::PRIMARY),
                            text(if cache.hit_rate_percent.is_some() {
                                "Cached input / reported input"
                            } else {
                                "No token usage reported yet"
                            })
                            .size(13)
                            .color(appearance::MUTED),
                            text(format!(
                                "Cached {} · Input {} · Output {} · Cache writes {}",
                                cache.cached_input_tokens,
                                cache.input_tokens,
                                cache.output_tokens,
                                cache.cache_creation_input_tokens
                            ))
                            .size(12),
                        ]
                        .spacing(7),
                    )
                    .padding(20)
                    .width(Length::Fill)
                    .style(|theme| appearance::message(theme, false)),
                );
            }
            if self.audit_items.is_empty() && !self.audit_busy {
                entries = entries.push(text("No tool calls yet.").size(16));
            }
            for run in &self.audit_items {
                let mut details = column![
                    button(text(format!("{} · {}", run.name, run.status)).size(15))
                        .style(appearance::ghost)
                        .on_press(Event::AuditToggle(run.id.clone())),
                    text(format!("{} · {}", run.thread_title, run.created_at))
                        .size(12)
                        .color(appearance::MUTED),
                ]
                .spacing(8);
                if self.audit_expanded.as_deref() == Some(&run.id) {
                    let arguments =
                        serde_json::to_string_pretty(&run.arguments).unwrap_or_default();
                    let result = run
                        .result
                        .as_ref()
                        .map(|value| serde_json::to_string_pretty(value).unwrap_or_default())
                        .unwrap_or_else(|| "Pending".into());
                    details = details
                        .push(text("Arguments").size(12))
                        .push(text(arguments).size(12))
                        .push(text("Result").size(12))
                        .push(text(result).size(12))
                        .push(
                            text(format!(
                                "Call ID: {} · Started: {} · Finished: {}",
                                run.call_id,
                                run.started_at.as_deref().unwrap_or("Pending"),
                                run.finished_at.as_deref().unwrap_or("Pending")
                            ))
                            .size(11)
                            .color(appearance::MUTED),
                        );
                }
                entries = entries.push(
                    container(details)
                        .padding(16)
                        .width(Length::Fill)
                        .style(|theme| appearance::message(theme, false)),
                );
            }
            if !self.audit_error.is_empty() {
                entries = entries.push(text(&self.audit_error).size(14));
            }
            let controls = row![
                button("Previous").style(appearance::ghost).on_press_maybe(
                    (!self.audit_busy && self.audit_page > 0).then_some(Event::AuditPrevious)
                ),
                text(format!("Page {}", self.audit_page + 1)).size(13),
                button("Next").style(appearance::ghost).on_press_maybe(
                    (!self.audit_busy && self.audit_next.is_some()).then_some(Event::AuditNext)
                ),
                if self.audit_busy {
                    text("Loading…").size(12)
                } else {
                    text("").size(12)
                },
            ]
            .spacing(14)
            .align_y(iced::Alignment::Center);
            let content = column![scrollable(entries).height(Length::Fill), controls].spacing(18);
            return row![
                sidebar,
                container(content)
                    .padding(40)
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .height(Length::Fill)
            .into();
        }

        if self.plugins_open {
            let plugins = &self.session.plugins;
            let mut content = column![
                text("Plugins").size(36),
                text("Workspace plugins · discovered automatically")
                    .size(14)
                    .color(appearance::MUTED),
                text(&plugins.directory).size(11).color(appearance::MUTED)
            ]
            .spacing(18);
            if plugins.items.is_empty() {
                content = content.push(text("No plugins installed in this workspace.").size(16));
            }
            for plugin in &plugins.items {
                let mut details = column![
                    text(&plugin.name).size(22),
                    text(plugin.description.as_deref().unwrap_or("")).size(14),
                    text(format!(
                        "{} · {} {} · {} MCP {}",
                        plugin.path,
                        plugin.skills.len(),
                        if plugin.skills.len() == 1 {
                            "skill"
                        } else {
                            "skills"
                        },
                        plugin.mcp_servers.len(),
                        if plugin.mcp_servers.len() == 1 {
                            "server"
                        } else {
                            "servers"
                        }
                    ))
                    .size(12)
                    .color(appearance::MUTED)
                ]
                .spacing(10);
                for issue in &plugin.issues {
                    details = details.push(
                        text(format!("Not loaded: {} · {}", issue.path, issue.message)).size(13),
                    );
                }
                content = content.push(
                    container(details)
                        .padding(20)
                        .width(Length::Fill)
                        .style(|theme| appearance::message(theme, false)),
                );
            }
            for issue in &plugins.issues {
                content = content.push(text(format!("Not loaded: {}", issue.path)).size(14));
                content = content.push(text(&issue.message).size(14));
            }
            content = content.push(
                text("Install plugin folders in .agents/plugins/; each needs plugin.json.")
                    .size(13)
                    .color(appearance::MUTED),
            );
            let content = column![
                button("Back to conversation")
                    .padding(14)
                    .style(appearance::ghost)
                    .on_press(Event::CloseSettings),
                scrollable(content).height(Length::Fill)
            ]
            .spacing(14);
            return row![
                sidebar,
                container(content)
                    .padding(40)
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .height(Length::Fill)
            .into();
        }
        if self.mcp_open {
            let mcp = &self.session.mcp;
            let mut content = column![
                text("MCP servers").size(36),
                text("Workspace tools · connected automatically")
                    .size(14)
                    .color(appearance::MUTED),
                text(&mcp.workspace).size(11).color(appearance::MUTED)
            ]
            .spacing(18);
            if mcp.servers.is_empty() {
                content =
                    content.push(text("No MCP servers configured in this workspace.").size(16));
            }
            for server in &mcp.servers {
                let mut details = column![
                    text(&server.name).size(22),
                    text(format!(
                        "{} · {} · {} tools",
                        server.status,
                        server.transport,
                        server.tools.len()
                    ))
                    .size(14),
                    text(format!(
                        "{} · protocol {}",
                        server.source,
                        server
                            .protocol_version
                            .as_deref()
                            .unwrap_or("not connected")
                    ))
                    .size(12)
                    .color(appearance::MUTED)
                ]
                .spacing(10);
                if let Some(error) = &server.error {
                    details = details.push(text(error).size(14));
                }
                for tool in &server.tools {
                    details = details.push(text(&tool.name).size(14));
                    details =
                        details.push(text(&tool.description).size(13).color(appearance::MUTED));
                }
                content = content.push(
                    container(details)
                        .padding(20)
                        .width(Length::Fill)
                        .style(|theme| appearance::message(theme, false)),
                );
            }
            for issue in &mcp.issues {
                content = content.push(text(format!("Not loaded: {}", issue.path)).size(14));
                content = content.push(text(&issue.message).size(14));
            }
            content = content.push(
                text("Configure servers in .mcp.json or mcp.json. Changes apply automatically.")
                    .size(13)
                    .color(appearance::MUTED),
            );
            let content = column![
                button("Back to conversation")
                    .padding(14)
                    .style(appearance::ghost)
                    .on_press(Event::CloseSettings),
                scrollable(content).height(Length::Fill),
            ]
            .spacing(14);
            return row![
                sidebar,
                container(content)
                    .padding(40)
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .height(Length::Fill)
            .into();
        }
        if self.profile_open {
            let editor = text_editor(&self.profile)
                .placeholder("Edit system prompt")
                .on_action(Event::ProfileEdited)
                .padding(18)
                .height(Length::Fill);
            let content = column![
                text("MAKE SOLMU YOURS").size(11).color(appearance::MUTED),
                text("Profile").size(40),
                text("Choose how Solmu approaches your conversations.")
                    .size(16)
                    .color(appearance::MUTED),
                text("System prompt").size(14),
                editor,
                text("Shared across all threads and clients. Applies to subsequent replies.")
                    .size(12)
                    .color(appearance::MUTED),
                text(format!("Edited on {}", self.profile_edited_at))
                    .size(12)
                    .color(appearance::MUTED),
                text("Model (optional)").size(14),
                text_input(
                    &self.profile_backend_default.as_ref().map_or_else(
                        || "No model configured".into(),
                        |model| format!("{model} (default)")
                    ),
                    &self.profile_model
                )
                .padding(14)
                .style(appearance::input)
                .on_input(Event::ProfileModel),
                text("Leave empty to use the backend default. Thread overrides take priority.")
                    .size(12)
                    .color(appearance::MUTED),
                text(&self.settings_notice)
                    .size(13)
                    .color(appearance::PRIMARY),
                row![
                    button("Save profile")
                        .padding(14)
                        .style(appearance::primary)
                        .on_press_maybe(
                            (!self.settings_busy && !self.profile.text().trim().is_empty())
                                .then_some(Event::SaveProfile)
                        ),
                    button("Back to conversation")
                        .padding(14)
                        .style(appearance::ghost)
                        .on_press(Event::CloseSettings)
                ]
                .spacing(12)
            ]
            .spacing(18);
            return row![
                sidebar,
                container(content)
                    .padding(40)
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .height(Length::Fill)
            .into();
        }
        if self.skills_open {
            let skills = &self.session.skills;
            let mut content = column![
                text("Workspace skills").size(36),
                text("Discovered automatically in .agents/skills/.")
                    .size(14)
                    .color(appearance::MUTED),
                text(&skills.directory).size(11).color(appearance::MUTED),
            ]
            .spacing(18);
            if skills.items.is_empty() {
                content = content.push(text("No skills installed in this workspace.").size(16));
            }
            for skill in &skills.items {
                let mut details = column![
                    text(&skill.name).size(20),
                    text(&skill.description).size(14),
                    text(&skill.path).size(11).color(appearance::MUTED)
                ]
                .spacing(10);
                if let Some(compatibility) = &skill.compatibility {
                    details = details.push(text(format!("Requires: {compatibility}")).size(12));
                }
                content = content.push(
                    container(details)
                        .padding(20)
                        .width(Length::Fill)
                        .style(|theme| appearance::message(theme, false)),
                );
            }
            for issue in &skills.issues {
                content = content
                    .push(text(format!("Not loaded: {}\n{}", issue.path, issue.message)).size(13));
            }
            content = content.push(
                text("Install skill folders in .agents/skills/; each needs SKILL.md.")
                    .size(13)
                    .color(appearance::MUTED),
            );
            content = content.push(
                button("Back to conversation")
                    .padding(14)
                    .style(appearance::ghost)
                    .on_press(Event::CloseSettings),
            );
            return row![
                sidebar,
                container(scrollable(content))
                    .padding(40)
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .height(Length::Fill)
            .into();
        }
        if self.model_open {
            let enabled = !self.settings_busy && !self.session.busy;
            let mut choices = column![
                text("Model for this thread").size(36),
                text("Changes apply to the next reply.")
                    .size(14)
                    .color(appearance::MUTED),
                button("Default model")
                    .padding(14)
                    .style(appearance::ghost)
                    .on_press_maybe(enabled.then_some(Event::Action(Action::Model(None))))
            ]
            .spacing(14);
            if let Some(catalog) = &self.catalog {
                choices = choices.push(
                    text(
                        catalog
                            .provider
                            .as_deref()
                            .unwrap_or("No provider configured"),
                    )
                    .size(12)
                    .color(appearance::MUTED),
                );
                for model in &catalog.models {
                    choices = choices.push(
                        button(text(&model.name).size(16))
                            .padding(14)
                            .width(Length::Fill)
                            .style(appearance::ghost)
                            .on_press_maybe(
                                enabled
                                    .then(|| Event::Action(Action::Model(Some(model.id.clone())))),
                            ),
                    );
                }
            }
            choices = choices
                .push(
                    text_input("Custom model ID", &self.custom_model)
                        .style(appearance::input)
                        .on_input(Event::CustomModel)
                        .padding(14),
                )
                .push(
                    button("Apply model")
                        .padding(14)
                        .style(appearance::primary)
                        .on_press_maybe((enabled && !self.custom_model.trim().is_empty()).then(
                            || Event::Action(Action::Model(Some(self.custom_model.trim().into()))),
                        )),
                )
                .push(text(&self.settings_notice).size(13))
                .push(
                    button("Cancel")
                        .style(appearance::ghost)
                        .on_press(Event::CloseSettings),
                );
            return row![
                sidebar,
                container(choices)
                    .padding(40)
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .height(Length::Fill)
            .into();
        }

        let mut history = column![].spacing(22).padding(12);
        if self.session.messages.is_empty() {
            history = history.push(
                container(
                    column![
                        text("A little space for your\nnext big idea.").size(36),
                        text("Start a conversation. Follow the thread.")
                            .size(17)
                            .color(appearance::MUTED),
                        text("Your history stays with you, in every Solmu client.")
                            .size(13)
                            .color(appearance::MUTED)
                    ]
                    .spacing(18),
                )
                .padding([64, 28]),
            );
        }
        for message in &self.session.messages {
            let role = if message.role == "user" {
                "YOU"
            } else {
                "SOLMU"
            };
            history = history.push(
                container(
                    column![
                        text(role).size(11).color(if message.role == "user" {
                            appearance::MUTED
                        } else {
                            appearance::PRIMARY
                        }),
                        text(&message.content).size(17)
                    ]
                    .spacing(8),
                )
                .padding(20)
                .width(Length::Fill)
                .style(move |theme| appearance::message(theme, message.role == "user")),
            );
            for tool in self
                .session
                .tools
                .iter()
                .filter(|tool| tool.message_id == message.id)
            {
                history = history.push(
                    container(
                        column![
                            text(format!("{} · {}", tool.name, tool.status))
                                .size(12)
                                .color(appearance::PRIMARY),
                            text(tool.details()).size(13).font(iced::Font::MONOSPACE),
                        ]
                        .spacing(8),
                    )
                    .padding(16)
                    .width(Length::Fill)
                    .style(|theme| appearance::message(theme, false)),
                );
            }
        }
        if !self.session.partial.is_empty() {
            history = history.push(
                container(
                    column![
                        text("SOLMU · streaming")
                            .size(11)
                            .color(appearance::PRIMARY),
                        text(&self.session.partial).size(17)
                    ]
                    .spacing(8),
                )
                .padding(20)
                .width(Length::Fill)
                .style(|theme| appearance::message(theme, false)),
            );
        }
        let has_thread = self.session.current.is_some();
        let heading = row![
            text_input("Conversation title", &self.title)
                .style(appearance::input)
                .on_input(Event::Title)
                .width(Length::Fill),
            button("Rename").style(appearance::ghost).on_press_maybe(
                (enabled && has_thread && !self.title.trim().is_empty())
                    .then(|| Event::Action(Action::Rename(self.title.clone())))
            ),
            button("Delete")
                .style(appearance::danger)
                .on_press_maybe((enabled && has_thread).then_some(Event::Action(Action::Delete))),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        let model_label = self
            .session
            .current
            .as_ref()
            .and_then(|thread| thread.model.as_deref())
            .unwrap_or("Default model");
        let model = row![
            button(text("Plugins").size(12))
                .style(appearance::ghost)
                .on_press_maybe(
                    (enabled && self.session.current.is_some()).then_some(Event::OpenPlugins)
                ),
            button(text("MCP").size(12))
                .style(appearance::ghost)
                .on_press_maybe(
                    (enabled && self.session.current.is_some()).then_some(Event::OpenMcp)
                ),
            button(text("Skills").size(12))
                .style(appearance::ghost)
                .on_press_maybe(
                    (enabled && self.session.current.is_some()).then_some(Event::OpenSkills)
                ),
            button(text(model_label).size(12))
                .style(appearance::ghost)
                .on_press_maybe(
                    (enabled && self.session.current.is_some()).then_some(Event::OpenModels)
                ),
            text(
                self.session
                    .current
                    .as_ref()
                    .and_then(|thread| thread.workspace.as_deref())
                    .unwrap_or("")
            )
            .size(11)
            .color(appearance::MUTED)
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center);
        let status = self
            .session
            .error
            .as_deref()
            .unwrap_or(if self.session.busy {
                "Solmu is working…"
            } else if !self.connected {
                "Reconnecting to live updates…"
            } else {
                "Ready"
            });
        let send: Element<'_, Event> = if self.session.responding {
            button(text("Stop ■"))
                .padding(16)
                .style(appearance::danger)
                .on_press(Event::Stop)
                .into()
        } else {
            button(text("Send ↑"))
                .style(appearance::primary)
                .padding(16)
                .on_press_maybe(
                    (enabled && has_thread && !self.draft.trim().is_empty()).then_some(Event::Send),
                )
                .into()
        };
        let composer = row![
            text_input("Message Solmu…", &self.draft)
                .style(appearance::input)
                .on_input(Event::Draft)
                .on_submit(Event::Send)
                .padding(16)
                .width(Length::Fill),
            send,
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center);
        let main = container(
            column![
                heading,
                model,
                scrollable(history).height(Length::Fill),
                text(status)
                    .size(12)
                    .color(if self.session.error.is_some() {
                        appearance::DANGER
                    } else {
                        appearance::MUTED
                    }),
                composer,
                text("Enter to send · Provider credentials stay on your backend")
                    .size(11)
                    .color(appearance::MUTED)
            ]
            .spacing(16),
        )
        .padding(32)
        .width(Length::Fill)
        .height(Length::Fill);
        row![sidebar, main].height(Length::Fill).into()
    }
}
