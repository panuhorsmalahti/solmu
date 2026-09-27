use serde_json::json;
use std::io::{BufRead, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(index) = args.iter().position(|value| value == "--report") {
        let path = args.get(index + 1).ok_or("Report path is required")?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(
            file,
            "{}",
            json!({"argv":args,"cwd":std::env::current_dir()?.canonicalize()?,"pane":std::env::var("SOLMU_MUXER_PANE_ID")?,"bin":std::env::var("SOLMU_MUXER_BIN")?,"agent_token":std::env::var("SOLMU_MUXER_AGENT_TOKEN").ok()})
        )?;
    }
    // A generic program must never acquire Solmu's lifecycle authority by title.
    print!("\x1b]2;Solmu | idle | 00000000-0000-0000-0000-000000000000\x07");
    println!("LOCAL TERMINAL READY");
    std::io::stdout().flush()?;
    if args.iter().any(|value| value == "--once") {
        return Ok(());
    }
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line == "quit" {
            return Ok(());
        }
        println!("INPUT {line}");
        std::io::stdout().flush()?;
    }
    Ok(())
}
