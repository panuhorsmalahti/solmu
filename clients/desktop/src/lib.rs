use futures_util::StreamExt;
use iced::{
    Element, Length, Task, Theme,
    widget::{button, column, container, row, scrollable, text, text_input, tooltip},
};
use solmu_client::{Action, Api, Connection, Session, Update};

mod appearance;
pub use appearance::theme;

pub struct Desktop {
    pub session: Session,
    draft: String,
    title: String,
    connected: bool,
    pending_refresh: bool,
    active: Option<iced::task::Handle>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Action(Action),
    Updated(Update),
    Draft(String),
    Title(String),
    Send,
    Stop,
    Connection(Connection),
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
        match event {
            Event::Stop => {
                if !self.session.responding {
                    return Task::none();
                }
                if let Some(handle) = self.active.take() {
                    handle.abort();
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
                Task::perform(async move { api.stop(&id).await }, |result| {
                    Event::Updated(match result {
                        Ok(()) => Update::Stopped,
                        Err(error) => Update::Failed(error),
                    })
                })
            }
            Event::Connection(change) => {
                match change {
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
            Event::Action(action) => self.act(action),
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
        let status = self
            .session
            .error
            .as_deref()
            .unwrap_or(if self.session.busy {
                "Solmu is working…"
            } else if !self.connected {
                "Reconnecting to live updates…"
            } else {
                "Ready · conversations saved locally"
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
