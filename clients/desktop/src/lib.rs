use iced::{Element, Length, Task, Theme, widget::{button, column, container, row, scrollable, text, text_input, tooltip}};
use solmu_client::{Action, Api, Session, Update};

pub struct Desktop {
    pub session: Session,
    draft: String,
    title: String,
}

#[derive(Debug, Clone)]
pub enum Event { Action(Action), Updated(Update), Draft(String), Title(String), Send }

pub fn application(api: Api) -> iced::Application<impl iced::Program<State = Desktop, Message = Event, Theme = Theme>> {
    iced::application(move || Desktop::new(api.clone()), Desktop::update, Desktop::view)
        .title("Solmu")
        .theme(Theme::TokyoNight)
        .window_size((1120.0, 760.0))
}

impl Desktop {
    pub fn new(api: Api) -> (Self, Task<Event>) {
        let mut desktop = Self { session: Session::new(api), draft: String::new(), title: String::new() };
        let task = desktop.act(Action::New("New conversation".into()));
        (desktop, task)
    }

    fn act(&mut self, action: Action) -> Task<Event> {
        if let Some(updates) = self.session.begin(action) { Task::run(updates, Event::Updated) } else { Task::none() }
    }

    pub fn update(&mut self, event: Event) -> Task<Event> {
        match event {
            Event::Action(action) => self.act(action),
            Event::Updated(update) => {
                let opened = matches!(update, Update::Opened(_, _));
                self.session.apply(update);
                if opened { self.title = self.session.current.as_ref().map(|thread| thread.title.clone()).unwrap_or_default(); self.draft.clear(); }
                Task::none()
            },
            Event::Draft(value) => { self.draft = value; Task::none() },
            Event::Title(value) => { self.title = value; Task::none() },
            Event::Send => {
                if self.session.busy || self.session.current.is_none() || self.draft.trim().is_empty() { return Task::none(); }
                let content = std::mem::take(&mut self.draft);
                self.act(Action::Send(content))
            },
        }
    }

    pub fn view(&self) -> Element<'_, Event> {
        let enabled = !self.session.busy;
        let new = tooltip(button(text("+").size(27)).on_press_maybe(enabled.then(|| Event::Action(Action::New("New conversation".into())))), "New thread", tooltip::Position::Bottom);
        let mut list = column![row![text("Conversations").size(13), iced::widget::space().width(Length::Fill), new].align_y(iced::Alignment::Center)].spacing(12);
        for thread in &self.session.threads {
            let selected = self.session.current.as_ref().is_some_and(|current| current.id == thread.id);
            list = list.push(button(text(&thread.title).size(14)).width(Length::Fill).padding(12).style(if selected { button::primary } else { button::text }).on_press_maybe(enabled.then(|| Event::Action(Action::Open(thread.id.clone())))));
        }
        let sidebar = container(column![
            text("solmu").size(32), text("A space to think.").size(13), iced::widget::space().height(22),
            scrollable(list).height(Length::Fill), button("Refresh").style(button::text).on_press_maybe(enabled.then_some(Event::Action(Action::Refresh))),
            text("YOUR IDEAS, CONNECTED").size(10)
        ].spacing(8)).padding(24).width(270).height(Length::Fill).style(container::rounded_box);

        let mut history = column![].spacing(22).padding(12);
        if self.session.messages.is_empty() {
            history = history.push(container(column![text("A little space for your\nnext big idea.").size(36), text("Start a conversation. Follow the thread.").size(17), text("Your history stays with you, in every Solmu client.").size(13)].spacing(18)).padding([64, 28]));
        }
        for message in &self.session.messages {
            let role = if message.role == "user" { "YOU" } else { "SOLMU" };
            history = history.push(container(column![text(role).size(11), text(&message.content).size(17)].spacing(8)).padding(20).width(Length::Fill).style(container::rounded_box));
        }
        if !self.session.partial.is_empty() { history = history.push(container(column![text("SOLMU · streaming").size(11), text(&self.session.partial).size(17)].spacing(8)).padding(20).width(Length::Fill).style(container::rounded_box)); }
        let has_thread = self.session.current.is_some();
        let heading = row![
            text_input("Conversation title", &self.title).on_input(Event::Title).width(Length::Fill),
            button("Rename").style(button::text).on_press_maybe((enabled && has_thread && !self.title.trim().is_empty()).then(|| Event::Action(Action::Rename(self.title.clone())))),
            button("Delete").style(button::danger).on_press_maybe((enabled && has_thread).then_some(Event::Action(Action::Delete))),
        ].spacing(8).align_y(iced::Alignment::Center);
        let status = self.session.error.as_deref().unwrap_or(if self.session.busy { "Solmu is working…" } else { "Ready · conversations saved locally" });
        let composer = row![
            text_input("Message Solmu…", &self.draft).on_input(Event::Draft).on_submit(Event::Send).padding(16).width(Length::Fill),
            button(text("Send ↑")).padding(16).on_press_maybe((enabled && has_thread && !self.draft.trim().is_empty()).then_some(Event::Send)),
        ].spacing(12).align_y(iced::Alignment::Center);
        let main = container(column![heading, scrollable(history).height(Length::Fill), text(status).size(12), composer, text("Enter to send · Provider credentials stay on your backend").size(11)].spacing(16)).padding(32).width(Length::Fill).height(Length::Fill);
        row![sidebar, main].height(Length::Fill).into()
    }
}
