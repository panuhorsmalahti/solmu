use futures_util::StreamExt;
use iced::{Program, Size, Theme};
use iced_test::{
    Emulator, Instruction,
    emulator::{Event, Mode},
};
use solmu_client::Api;
use solmu_e2e::support::Backend;
use std::time::Duration;

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
                        .find(text)
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktop_thread_sidebar_streaming_history_crud_and_errors() {
    let backend = Backend::start().await;
    let mut ui =
        tokio::task::block_in_place(|| Ui::new(solmu_desktop::application(Api::new(&backend.url))));
    ui.wait("Ready · conversations saved locally").await;
    let first = backend.threads().await;
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    let id = first["items"][0]["id"].as_str().unwrap();
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"Hello from the desktop\"").await;
    ui.step("click \"Send ↑\"").await;
    ui.wait("SOLMU · streaming").await;
    ui.wait("Hello").await;
    assert_eq!(
        backend.messages(id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ui.wait("Hello from Solmu").await;
    ui.wait("Ready · conversations saved locally").await;
    ui.step("click (460, 52)").await;
    for _ in 0..16 {
        ui.step("type backspace").await;
    }
    ui.step("type \"Desktop planning\"").await;
    ui.step("click \"Rename\"").await;
    ui.wait("Desktop planning").await;
    ui.wait("Ready · conversations saved locally").await;
    ui.step("click \"+\"").await;
    ui.wait("A little space for your\nnext big idea.").await;
    ui.wait("Ready · conversations saved locally").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        2
    );
    ui.step("click \"Desktop planning\"").await;
    ui.wait("Hello from the desktop").await;
    ui.wait("Ready · conversations saved locally").await;
    ui.step("click \"Delete\"").await;
    ui.wait("A little space for your\nnext big idea.").await;
    ui.wait("Ready · conversations saved locally").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
    ui.step("click \"New conversation\"").await;
    ui.wait("Ready · conversations saved locally").await;
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"FAIL\"").await;
    ui.step("type enter").await;
    ui.wait("LLM request failed; check provider credentials, model, and endpoint")
        .await;
    backend.create_thread("From another client").await;
    ui.wait("From another client").await;
    ui.wait("FAIL").await;
    ui.wait_enabled("From another client").await;
    ui.step("click \"From another client\"").await;
    ui.wait("Ready · conversations saved locally").await;
    let stop_id = backend.threads().await["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|thread| thread["title"] == "From another client")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"A new idea\"").await;
    ui.step("type enter").await;
    ui.wait("Hello").await;
    ui.step("click \"Stop ■\"").await;
    ui.wait("Ready · conversations saved locally").await;
    assert_eq!(
        backend.messages(&stop_id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ui.step("click \"Message Solmu…\"").await;
    ui.step("type \"Hello from the desktop\"").await;
    ui.step("type enter").await;
    ui.wait("Hello from Solmu").await;
    ui.wait("Ready · conversations saved locally").await;
    // iced_test::Emulator::screenshot consumes its layout cache. Capture last.
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        let screenshot =
            ui.emulator
                .as_mut()
                .unwrap()
                .screenshot(&ui.program, &Theme::TokyoNight, 1.0);
        let path = solmu_e2e::support::root().join("docs/screenshots/desktop.png");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut encoder = png::Encoder::new(
            std::fs::File::create(path).unwrap(),
            screenshot.size.width,
            screenshot.size.height,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&screenshot.rgba)
            .unwrap();
    }
}
