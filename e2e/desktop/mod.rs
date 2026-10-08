use futures_util::StreamExt;
use iced::{Program, Size};
use iced_test::{
    Emulator, Instruction,
    emulator::{Event, Mode},
};
use solmu_client::Api;
use solmu_e2e::support::Backend;
use std::time::Duration;

struct ContainsText<'a>(&'a str);

impl iced_test::selector::Selector for ContainsText<'_> {
    type Output = ();

    fn select(&mut self, candidate: iced_test::selector::Candidate<'_>) -> Option<Self::Output> {
        match candidate {
            iced_test::selector::Candidate::Text { content, .. } if content.contains(self.0) => {
                Some(())
            }
            iced_test::selector::Candidate::TextInput { state, .. }
                if state.text().contains(self.0) =>
            {
                Some(())
            }
            _ => None,
        }
    }

    fn description(&self) -> String {
        format!("text contains {:?}", self.0)
    }
}

struct Ui<P: Program> {
    program: P,
    emulator: Option<Emulator<P>>,
    receiver: futures_util::stream::BoxStream<'static, Event<P>>,
}
impl<P: Program + 'static> Ui<P> {
    fn new(program: P) -> Self {
        let (sender, receiver) = iced_test::futures::futures::channel::mpsc::channel(100);
        let emulator = Emulator::new(sender, &program, Mode::Immediate, Size::new(1120.0, 760.0));
        Self {
            program,
            emulator: Some(emulator),
            receiver: receiver.boxed(),
        }
    }
    async fn step(&mut self, instruction: &str) {
        eprintln!("Desktop: {instruction}");
        self.emulator
            .as_mut()
            .unwrap()
            .run(&self.program, Instruction::parse(instruction).unwrap());
        self.pump_ready().await;
    }
    async fn pump_ready(&mut self) {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                match self.receiver.next().await.unwrap() {
                    Event::Action(action) => self
                        .emulator
                        .as_mut()
                        .unwrap()
                        .perform(&self.program, action),
                    Event::Ready => break,
                    Event::Failed(instruction) => {
                        panic!("Desktop interaction failed: {instruction}")
                    }
                }
            }
        })
        .await
        .unwrap();
    }
    async fn wait(&mut self, text: &str) {
        eprintln!("Desktop: waiting for {text:?}");
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let found =
                    iced_test::simulator(self.emulator.as_ref().unwrap().view(&self.program))
                        .find(ContainsText(text))
                        .is_ok();
                if found {
                    break;
                }
                match self.receiver.next().await.unwrap() {
                    Event::Action(action) => self
                        .emulator
                        .as_mut()
                        .unwrap()
                        .perform(&self.program, action),
                    Event::Ready => {}
                    Event::Failed(instruction) => {
                        panic!("Desktop interaction failed: {instruction}")
                    }
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("Desktop did not show {text:?}"));
    }
    async fn wait_enabled(&mut self, label: &str) {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let enabled = {
                    let mut simulator =
                        iced_test::simulator(self.emulator.as_ref().unwrap().view(&self.program));
                    simulator.click(label).is_ok() && simulator.into_messages().next().is_some()
                };
                if enabled {
                    break;
                }
                match self.receiver.next().await.unwrap() {
                    Event::Action(action) => self
                        .emulator
                        .as_mut()
                        .unwrap()
                        .perform(&self.program, action),
                    Event::Ready => {}
                    Event::Failed(instruction) => {
                        panic!("Desktop interaction failed: {instruction}")
                    }
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("Desktop button did not become enabled: {label}"));
    }
}

impl<P: Program> Drop for Ui<P> {
    fn drop(&mut self) {
        if let Some(emulator) = self.emulator.take() {
            tokio::task::block_in_place(|| drop(emulator));
        }
    }
}

mod audit;
mod commands;
mod compaction;
mod conversations;
mod goals;
mod mcp;
mod memories;
mod models;
mod muxer_embedded;
mod plugins;
mod profile;
mod screenshots;
mod skills;
mod tasks;
mod tools;
