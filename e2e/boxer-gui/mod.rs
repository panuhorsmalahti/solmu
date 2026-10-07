use futures_util::StreamExt;
use iced::{Program, Size};
use iced_test::{
    Emulator,
    emulator::{Event, Mode},
};
use solmu_boxer_gui::application_with_root;
use std::{path::Path, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_session_files_update_the_gui_and_capture_its_screenshot() {
    let sessions = tempfile::tempdir().unwrap();
    let id = "11111111-1111-4111-8111-111111111111";
    let record = serde_json::json!({
        "version": 1,
        "id": id,
        "pid": 42,
        "created_unix_ms": 1_800_000_000_000u64,
        "workspace": "C:/projects/solmu",
        "command": "fixture-agent",
        "status": "running",
        "detached": true,
        "attached": false,
        "exit_code": null
    });
    std::fs::write(
        sessions.path().join(format!("{id}.json")),
        serde_json::to_vec_pretty(&record).unwrap(),
    )
    .unwrap();

    let program = application_with_root(sessions.path().to_owned());
    let (sender, receiver) = iced_test::futures::futures::channel::mpsc::channel(100);
    let mut emulator = tokio::task::block_in_place(|| {
        Emulator::new(sender, &program, Mode::Immediate, Size::new(980.0, 720.0))
    });
    let mut receiver = receiver.boxed();
    wait_for(&mut emulator, &program, &mut receiver, "1 tracked").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        let screenshot = emulator.screenshot(&program, &solmu_desktop::theme(), 1.0);
        let path = solmu_e2e::support::root().join("docs/screenshots/boxer-gui.png");
        save_screenshot(
            &path,
            screenshot.size.width,
            screenshot.size.height,
            &screenshot.rgba,
        );
    }
    tokio::task::block_in_place(|| drop(emulator));
}

async fn wait_for<P: Program + 'static>(
    emulator: &mut Emulator<P>,
    program: &P,
    receiver: &mut futures_util::stream::BoxStream<'static, Event<P>>,
    expected: &str,
) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if iced_test::simulator(emulator.view(program))
                .find(expected)
                .is_ok()
            {
                return;
            }
            match receiver.next().await.unwrap() {
                Event::Action(action) => emulator.perform(program, action),
                Event::Ready => {}
                Event::Failed(instruction) => panic!("Boxer GUI interaction failed: {instruction}"),
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("Boxer GUI did not show {expected:?}"));
}

fn save_screenshot(path: &Path, width: u32, height: u32, rgba: &[u8]) {
    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(rgba)
        .unwrap();
}
