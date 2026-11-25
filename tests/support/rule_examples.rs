use godot_llm_lint::features::{
    architecture::helpers::analyze_project as analyze, configuration::models::Project,
};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};
static INDEX: AtomicUsize = AtomicUsize::new(0);
#[allow(
    dead_code,
    reason = "Selected integration suites use this shared capture helper"
)]
pub fn analyze_project(
    project: &Project,
    targets: &[String],
) -> godot_llm_lint::shared::models::Outcome<godot_llm_lint::shared::models::ArchitectureReport> {
    let report = analyze(project, targets)?;
    record(project, "architecture", targets, &report.diagnostics);
    Ok(report)
}
pub fn record<T: serde::Serialize>(
    project: &Project,
    operation: &str,
    targets: &[String],
    diagnostics: &T,
) {
    let Some(destination) = std::env::var_os("LLM_LINT_CAPTURE_RULE_EXAMPLES") else {
        return;
    };
    let mut files = BTreeMap::new();
    collect(&project.root, &project.root, &mut files);
    let data = serde_json::json!({"testName": std::thread::current().name().unwrap_or(""), "operation": operation, "args": targets, "settings": project.settings, "files":files, "diagnostics": diagnostics});
    let root = project.root.to_string_lossy();
    let encoded = serde_json::to_string(root.as_ref()).unwrap();
    let output = serde_json::to_string(&data)
        .unwrap()
        .replace(&encoded[1..encoded.len() - 1], "__ROOT__")
        .replace(&root.replace('\\', "/"), "__ROOT__");
    fs::create_dir_all(&destination).unwrap();
    fs::write(
        Path::new(&destination).join(format!(
            "{}-{}.json",
            std::process::id(),
            INDEX.fetch_add(1, Ordering::Relaxed)
        )),
        output,
    )
    .unwrap();
}
fn collect(root: &Path, directory: &Path, files: &mut BTreeMap<String, String>) {
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        let filename = entry.path();
        if kind.is_symlink()
            || ["target", ".git", ".godot", ".tools", ".build"]
                .contains(&entry.file_name().to_string_lossy().as_ref())
        {
            continue;
        }
        if kind.is_dir() {
            collect(root, &filename, files);
        } else if filename.extension().is_some_and(|ext| {
            [
                "rs", "toml", "json", "godot", "gd", "tscn", "tres", "uid", "md",
            ]
            .contains(&ext.to_string_lossy().as_ref())
        }) && let Ok(text) = fs::read_to_string(&filename)
        {
            files.insert(
                filename
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                text,
            );
        }
    }
}

#[allow(
    dead_code,
    reason = "Only the duplicate integration suite uses this capture helper"
)]
pub fn analyze_duplicates(
    project: &Project,
    targets: &[String],
) -> godot_llm_lint::shared::models::Outcome<
    godot_llm_lint::features::duplicates::models::DuplicateReport,
> {
    let report =
        godot_llm_lint::features::duplicates::helpers::analyze_duplicates(project, targets)?;
    record(project, "duplicates", targets, &report.diagnostics);
    Ok(report)
}
