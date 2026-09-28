use serde_json::{Value, json};
use std::{io::Write, path::Path};

pub fn install(workspace: &Path, legacy: bool) -> std::path::PathBuf {
    std::fs::create_dir_all(workspace).unwrap();
    let path = workspace.join(".mcp.json");
    let config = json!({"mcpServers":{"notes":{"command":super::binary("mcp-fixture"),"args":if legacy {vec!["legacy"]} else {vec![]},"env":{"SOLMU_MCP_TEST":"configured"}}}});
    std::fs::write(&path, config.to_string()).unwrap();
    path
}
pub fn record(workspace: &Path, message: &Value) {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(workspace.join("mcp-requests.jsonl"))
        .unwrap();
    writeln!(file, "{message}").unwrap();
}
pub fn respond(request: &Value, workspace: &Path, legacy: bool) -> Option<Value> {
    let id = request.get("id")?;
    let method = request["method"].as_str().unwrap();
    let result = match method {
        "server/discover" if legacy => {
            return Some(
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}}),
            );
        }
        "server/discover" => {
            json!({"resultType":"complete","supportedVersions":["2026-07-28"],"capabilities":{"tools":{}},"_meta":{"io.modelcontextprotocol/serverInfo":{"name":"fixture","version":"1.0"}},"ttlMs":0,"cacheScope":"private"})
        }
        "initialize" => {
            json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"legacy-fixture","version":"1.0"}})
        }
        "tools/list" => {
            let description = std::fs::read_to_string(workspace.join("mcp-description.txt"))
                .unwrap_or_else(|_| "Search project notes with MCP.".into());
            let tools = if request["params"]["cursor"] == "second" {
                json!([{"name":"fail","description":"Return a tool error.","inputSchema":{"type":"object","properties":{}}},{"name":"slow","description":"Wait until cancelled.","inputSchema":{"type":"object","properties":{}}}])
            } else {
                json!([{"name":"search","description":description,"inputSchema":{"type":"object","properties":{"query":{"type":"string","x-mcp-header":"Query"}},"required":["query"]}}])
            };
            let mut result = json!({"tools":tools,"ttlMs":0,"cacheScope":"private"});
            if request["params"]["cursor"] != "second" {
                result["nextCursor"] = json!("second");
            }
            result
        }
        "tools/call" => {
            if request["params"]["name"] == "slow" {
                return None;
            }
            if request["params"]["name"] == "fail" {
                json!({"content":[{"type":"text","text":"Fixture tool failure"}],"isError":true})
            } else {
                json!({"content":[{"type":"text","text":"Found MCP project notes"}],"structuredContent":{"query":request["params"]["arguments"]["query"],"env":std::env::var("SOLMU_MCP_TEST").unwrap_or_default()},"isError":false,"_meta":{"private":"do not send to model"}})
            }
        }
        _ => {
            return Some(
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}}),
            );
        }
    };
    let mut result = result;
    if !legacy {
        result["resultType"] = json!("complete");
    }
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}
