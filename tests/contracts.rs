#![cfg(test)]
mod support;
use godot_llm_lint::features::architecture::helpers::analyze_project;
use support::{project, write};

#[test]
fn mapper_contracts_resolve_global_inner_preload_and_engine_types() {
    let root = tempfile::tempdir().unwrap();
    let mut project = project(root.path(), "");
    write(
        root.path(),
        "src/shared/models/user_model.gd",
        "class_name UserModel\nextends RefCounted\nclass UserDto:\n\textends RefCounted\n",
    );
    write(
        root.path(),
        "src/shared/mappers.gd",
        r#"extends RefCounted
const Renamed = preload("res://src/shared/models/user_model.gd")
const Alias = Renamed.UserDto
class Person:
	extends RefCounted
static func map_user_model() -> Renamed:
	return Renamed.new()
static func map_user_dto() -> Alias:
	return Alias.new()
static func map_user_dtos() -> Array[UserModel.UserDto]:
	return []
static func map_people() -> Array[Person]:
	return []
static func map_image() -> Image:
	return Image.new()
static func map_raw() -> Dictionary:
	return {}
static func map_color() -> Color:
	return Color.RED
static func map_wrong() -> UserModel:
	return UserModel.new()
static func map_missing() -> Missing:
	return null
static func map_unannotated():
	return UserModel.new()
"#,
    );
    project.settings.rules=serde_json::from_value(serde_json::json!({"architecture/mapper-return-type":{"options":{"pluralForms":{"Person":"People"}}}})).unwrap();
    let report = analyze_project(&project, &[]).unwrap();
    let naming: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.rule == "architecture/mapper-return-type")
        .collect();
    assert_eq!(naming.len(), 2, "{naming:?}");
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|d| d.rule == "architecture/mapper-output-declaration")
            .count(),
        1
    );
    project.settings.rules.insert(
        "architecture/mapper-output-declaration".into(),
        serde_json::from_value(serde_json::json!({"options":{"externalTypes":["Missing"]}}))
            .unwrap(),
    );
    assert!(
        !analyze_project(&project, &[])
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.rule == "architecture/mapper-output-declaration")
    );
}

#[test]
fn companion_rules_support_ordered_levels_and_disabled_pairs() {
    let root = tempfile::tempdir().unwrap();
    let mut project = project(root.path(), "");
    write(
        root.path(),
        "src/shared/mappers.gd",
        "extends RefCounted\nstatic func map_value() -> int:\n\treturn 1\n",
    );
    project.settings.rules=serde_json::from_value(serde_json::json!({"architecture/companion-files":{"options":{"pairs":{"note":{"match":"src/**/{name}.gd","require":["${name}.md"]}}}}})).unwrap();
    assert_eq!(
        analyze_project(&project, &[])
            .unwrap()
            .diagnostics
            .iter()
            .filter(|d| d.rule == "architecture/companion-files")
            .count(),
        1
    );
    project.settings.overrides=serde_json::from_value(serde_json::json!([{"includes":["src/**"],"rules":{"architecture/companion-files":{"level":"warn"}}}])).unwrap();
    assert!(
        analyze_project(&project, &[])
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.rule == "architecture/companion-files" && d.severity == "warning")
    );
    write(root.path(), "src/shared/mappers.md", "");
    assert!(
        !analyze_project(&project, &[])
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.rule == "architecture/companion-files")
    );
}
