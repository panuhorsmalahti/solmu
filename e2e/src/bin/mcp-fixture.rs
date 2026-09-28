use serde_json::Value;
use std::io::{BufRead, Write};

fn main() {
    let workspace = std::env::current_dir().unwrap();
    let legacy = std::env::args().any(|arg| arg == "legacy");
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let request: Value = serde_json::from_str(&line).unwrap();
        solmu_e2e::support::mcp::record(&workspace, &request);
        if request["method"] == "notifications/cancelled" {
            std::fs::write(workspace.join("mcp-cancelled"), "cancelled").unwrap();
        }
        if let Some(response) = solmu_e2e::support::mcp::respond(&request, &workspace, legacy) {
            println!("{response}");
            std::io::stdout().flush().unwrap();
        }
    }
}
