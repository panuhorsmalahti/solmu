use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn install(workspace: &Path, name: &str, description: &str) -> PathBuf {
    let directory = workspace.join(".agents/skills").join(name);
    fs::create_dir_all(directory.join("references")).unwrap();
    let skill = directory.join("SKILL.md");
    fs::write(&skill, format!("---\nname: {name}\ndescription: {description}\n---\n\n# Instructions\n\nKeep explanations clear and practical.\nSee references/style.md for the writing guide.\n")).unwrap();
    fs::write(
        directory.join("references/style.md"),
        "Use short sentences and concrete examples.",
    )
    .unwrap();
    skill
}
