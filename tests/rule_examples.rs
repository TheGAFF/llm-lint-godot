#![cfg(test)]
use godot_llm_lint::features::{
    architecture::helpers::analyze_project,
    configuration::{helpers::discover_project, rules::catalog},
    duplicates::helpers::analyze_duplicates,
};
use std::{fs, path::Path, process::Command};

#[test]
fn all_custom_and_selected_native_rules_have_verified_passing_and_failing_implementations() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/rules");
    for rule in catalog().unwrap() {
        if !["custom", "native", "formatting"].contains(&rule["source"].as_str().unwrap()) {
            continue;
        }
        let id = rule["id"].as_str().unwrap();
        let file = directory.join(format!("{}.json", id.replace('/', "--")));
        assert!(file.exists(), "Missing examples: {id}");
        let example: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
        for label in ["passing", "failing"] {
            verify_case(id, label, &example);
        }
    }
}

fn materialize(case: &serde_json::Value, root: &Path) {
    if let Some(directories) = case["directories"].as_array() {
        for directory in directories {
            fs::create_dir_all(root.join(directory.as_str().unwrap())).unwrap();
        }
    }
    for (file, text) in case["files"].as_object().unwrap() {
        assert!(!file.starts_with('/') && !file.split('/').any(|part| part == ".."));
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            text.as_str()
                .unwrap()
                .replace("__ROOT__", &root.to_string_lossy()),
        )
        .unwrap();
    }
}

fn verify_case(id: &str, label: &str, example: &serde_json::Value) {
    let case = &example[label];
    let root = tempfile::tempdir().unwrap();
    materialize(case, root.path());
    let diagnostics = case_diagnostics(case, root.path());
    let target: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["rule"] == id)
        .collect();
    assert_eq!(
        !target.is_empty(),
        label == "failing",
        "{id} {label}: {diagnostics:?}"
    );
    if label != "failing" || !example["expected"]["path"].is_string() {
        return;
    }
    assert!(
        target
            .iter()
            .any(|item| item["path"] == example["expected"]["path"]
                && item["line"] == example["expected"]["line"]),
        "{id}: wrong diagnostic location: {target:?}"
    );
}

fn case_diagnostics(case: &serde_json::Value, root: &Path) -> Vec<serde_json::Value> {
    let args: Vec<_> = case["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect();
    let operation = case["operation"].as_str().unwrap();
    if operation.starts_with("gdstyle") {
        return native_diagnostics(root, operation, &args);
    }
    let mut project = discover_project(root, None).unwrap();
    if !case["settings"].is_null() {
        project.settings = serde_json::from_value(case["settings"].clone()).unwrap();
    }
    let diagnostics = if operation == "duplicates" {
        serde_json::to_value(analyze_duplicates(&project, &args).unwrap().diagnostics).unwrap()
    } else {
        serde_json::to_value(analyze_project(&project, &args).unwrap().diagnostics).unwrap()
    };
    diagnostics.as_array().unwrap().clone()
}

fn native_diagnostics(root: &Path, operation: &str, args: &[String]) -> Vec<serde_json::Value> {
    let binary = std::env::var_os("GDSTYLE_TEST_BINARY").expect(
        "Set GDSTYLE_TEST_BINARY to the pinned executable; native verification cannot be skipped",
    );
    let output = Command::new(binary)
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        matches!(output.status.code(), Some(0 | 1)),
        "Native tool failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    if operation == "gdstyle-formatting" {
        return if output.status.success() {
            vec![]
        } else {
            vec![serde_json::json!({"rule":"formatting/check"})]
        };
    }
    let mut diagnostics: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    for diagnostic in &mut diagnostics {
        diagnostic["rule"] =
            serde_json::json!(format!("gdstyle/{}", diagnostic["rule"].as_str().unwrap()));
    }
    diagnostics
}

#[path = "support/documentation.rs"]
mod documentation;
#[test]
fn rule_documentation_is_current() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(
        fs::read_to_string(root.join("docs/rules.md")).unwrap(),
        documentation::render_rules(root),
        "Regenerate rule documentation with the ignored regeneration test"
    );
}
#[test]
#[ignore = "Explicit documentation regeneration writes docs/rules.md"]
fn regenerate_rule_documentation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(
        root.join("docs/rules.md"),
        documentation::render_rules(root),
    )
    .unwrap();
}
