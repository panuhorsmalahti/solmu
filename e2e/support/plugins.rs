use serde_json::json;
use std::path::{Path, PathBuf};

pub fn install(workspace: &Path, with_mcp: bool) -> PathBuf {
    let root = workspace.join(".agents/plugins/project-tools");
    let skill = root.join("skills/project-notes");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        root.join("plugin.json"),
        json!({
            "$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
            "name":"project-tools", "version":"1.0.0", "description":"Tools for this project"
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(skill.join("SKILL.md"), "---\nname: project-notes\ndescription: Read project notes from a plugin.\n---\n\nUse the MCP search tool.\n").unwrap();
    if with_mcp {
        let source = super::binary("mcp-fixture");
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let program = format!("mcp-fixture{}", std::env::consts::EXE_SUFFIX);
        std::fs::copy(source, bin.join(&program)).unwrap();
        std::fs::write(
            root.join("mcp.json"),
            json!({
                "$schema":"https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
                "mcpServers":{"notes":{"type":"stdio","command":format!("./bin/{program}"),
                    "env":{"SOLMU_MCP_TEST":"plugin-configured","PLUGIN_EXTRA":"${PLUGIN_DATA}/note"}}}
            })
            .to_string(),
        )
        .unwrap();
    }
    root
}
