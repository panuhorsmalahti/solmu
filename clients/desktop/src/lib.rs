use futures_util::StreamExt;
use iced::{
    Element, Length, Task, Theme,
    widget::{button, column, container, row, scrollable, text, text_editor, text_input, tooltip},
};
use solmu_client::{Action, Api, Connection, ModelCatalog, Profile, Session, Update};

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

    pub fn update(&mut self, event: Event) -> Task<Event> {
        let saved_profile = matches!(&event, Event::ProfileSaved(_));
        let opened_profile = matches!(&event, Event::OpenProfile);
        match event {
            Event::OpenSkills => {
                self.skills_open = true;
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
                    Connection::Connected | Connection::Changed => {
                        self.connected = true;
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
