#![cfg(test)]
use godot_llm_lint::features::configuration::{
    commands::sync_project, helpers::discover_project, models::Project,
};
use std::{fs, path::Path};

pub fn write(root: &Path, name: &str, content: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

pub fn project(root: &Path, config: &str) -> Project {
    write(root, "project.godot", "config_version=5\n");
    fs::create_dir_all(root.join("src")).unwrap();
    write(
        root,
        "llm-lint.godot.toml",
        &format!("version = 1\n{config}"),
    );
    let project = discover_project(root, None).unwrap();
    sync_project(&project, false).unwrap();
    project
}
