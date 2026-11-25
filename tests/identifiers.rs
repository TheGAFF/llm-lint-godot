#![cfg(test)]
mod support;
use godot_llm_lint::features::architecture::helpers::analyze_project;
use support::{project, write};

const CONFIG: &str = "[rules]\n'architecture/resource-id-name'='error'\n'architecture/resource-id-type'='error'\n'architecture/stringname-literal'='error'\n";
const BASE: &str =
    "class_name BaseResource\nextends Resource\n@export var id: StringName = &\"base\"\n";

fn resource(root: &std::path::Path, path: &str, class: &str, id: &str) {
    write(
        root,
        path,
        &format!(
            "[gd_resource type=\"Resource\" format=3]\n[ext_resource type=\"Script\" path=\"res://src/features/content/resources/{class}_resource.gd\" id=\"s\"]\n[resource]\nscript = ExtResource(\"s\")\n{id}\n"
        ),
    );
}

#[test]
fn inherited_ids_defaults_embedded_values_and_collisions() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "src/features/content/resources/base_resource.gd",
        BASE,
    );
    write(
        root.path(),
        "src/features/content/resources/child_resource.gd",
        "class_name ChildResource\nextends BaseResource\n",
    );
    resource(
        root.path(),
        "src/features/content/resources/base_resource.tres",
        "child",
        "",
    );
    resource(
        root.path(),
        "src/features/content/resources/other_resource.tres",
        "child",
        "id = &\"other\"",
    );
    let project = project(root.path(), CONFIG);
    let report = analyze_project(&project, &[]).unwrap();
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.rule.starts_with("architecture/resource-id")
                || d.rule == "architecture/stringname-literal"),
        "{:?}",
        report.diagnostics
    );
    resource(
        root.path(),
        "src/features/content/resources/other_resource.tres",
        "child",
        "id = \"base\"",
    );
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|d| d.rule == "architecture/resource-id-name")
            .count(),
        3
    );
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|d| d.rule == "architecture/stringname-literal")
            .count(),
        1
    );
    let report = analyze_project(
        &project,
        &["src/features/content/resources/other_resource.tres".into()],
    )
    .unwrap();
    assert!(report.diagnostics.iter().any(
        |d| d.rule == "architecture/resource-id-name" && d.path.ends_with("base_resource.tres")
    ));
    write(
        root.path(),
        "src/features/content/resources/embedded_resource.tres",
        "[gd_resource type=\"Resource\" format=3]\n[ext_resource type=\"Script\" path=\"res://src/features/content/resources/child_resource.gd\" id=\"s\"]\n[sub_resource type=\"Resource\" id=\"local\"]\nscript = ExtResource(\"s\")\nid = &\"embedded_without_filename\"\n[resource]\nscript = ExtResource(\"s\")\nid = &\"embedded\"\n",
    );
    let report = analyze_project(&project, &[]).unwrap();
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.path.ends_with("embedded_resource.tres")
                && d.rule == "architecture/resource-id-name")
    );
}

#[test]
fn numeric_and_nonidentity_fields_are_ignored_and_rules_are_opt_in() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "src/features/content/resources/base_resource.gd",
        "class_name BaseResource\nextends Resource\n@export var id: int = 1\n@export var effect_id: String = \"effect\"\n",
    );
    resource(
        root.path(),
        "src/features/content/resources/base_resource.tres",
        "base",
        "id = 123",
    );
    let configured = project(root.path(), CONFIG);
    assert!(
        !analyze_project(&configured, &[])
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.rule.contains("resource-id"))
    );
    write(
        root.path(),
        "src/features/content/resources/base_resource.gd",
        "class_name BaseResource\nextends Resource\n@export var id: String = \"wrong\"\n",
    );
    let configured = project(root.path(), "");
    assert!(
        !analyze_project(&configured, &[])
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.rule.contains("resource-id"))
    );
    let configured = project(root.path(), CONFIG);
    assert!(
        analyze_project(&configured, &[])
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.rule == "architecture/resource-id-type")
    );
}

#[test]
fn literal_contexts_resolve_scopes_project_calls_and_dynamic_conversions() {
    let root = tempfile::tempdir().unwrap();
    let path = "src/features/content/resources/base_resource.gd";
    write(
        root.path(),
        path,
        &format!(
            "{BASE}\nfunc take(key: StringName) -> StringName:\n\treturn key\nfunc check(other: BaseResource, dynamic: String) -> StringName:\n\tvar key: StringName = \"a\\\"b\"\n\tvar ids: Array[StringName] = [\"b\"]\n\tkey = \"c\"\n\ttake(\"d\")\n\tother.take(\"e\")\n\tif other.id == \"f\":\n\t\tpass\n\tif key in [\"g\"]:\n\t\tpass\n\tids.append(\"h\")\n\treturn \"i\"\nfunc safe(dynamic: String, unknown: Variant) -> StringName:\n\tvar key: StringName = StringName(dynamic)\n\tvar ids: Array[StringName] = [&\"b\"]\n\tunknown.take(\"unresolved\")\n\treturn &\"ok\"\n"
        ),
    );
    let configured = project(root.path(), CONFIG);
    let report = analyze_project(&configured, &[]).unwrap();
    let literals: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.rule == "architecture/stringname-literal")
        .collect();
    assert_eq!(literals.len(), 9, "{literals:?}");
    assert!(!literals.iter().any(|d| d.actual.contains("unresolved")));
}

#[test]
fn exact_next_line_suppressions_require_reasons_and_reject_unused_directives() {
    let root = tempfile::tempdir().unwrap();
    let path = "src/features/content/resources/base_resource.gd";
    write(
        root.path(),
        path,
        "class_name BaseResource\nextends Resource\n# llm-lint-ignore architecture/resource-id-type: External schema requires String.\n@export var id: String = \"base\"\n",
    );
    resource(
        root.path(),
        "src/features/content/resources/base_resource.tres",
        "base",
        "; llm-lint-ignore architecture/resource-id-name: Stable external contract.\nid = \"external\"",
    );
    let configured = project(root.path(), CONFIG);
    let report = analyze_project(&configured, &[]).unwrap();
    assert!(!report.diagnostics.iter().any(|d| d.rule.contains("resource-id") || d.rule == "architecture/comment-suppression"), "{:?}", report.diagnostics);
    for directive in [
        "architecture/resource-id-type:",
        "architecture/*: reason",
        "unknown/rule: reason",
        "architecture/resource-id-name: unused",
        "architecture/resource-id-type,architecture/resource-id-name: reason",
    ] {
        write(
            root.path(),
            path,
            &format!(
                "class_name BaseResource\nextends Resource\n# llm-lint-ignore {directive}\n@export var id: StringName = &\"base\"\n"
            ),
        );
        let report = analyze_project(&configured, &[]).unwrap();
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.rule == "architecture/comment-suppression"),
            "{directive}"
        );
    }
}

#[test]
fn levels_overrides_and_durable_exceptions_work() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "src/features/content/resources/base_resource.gd",
        BASE,
    );
    resource(
        root.path(),
        "src/features/content/resources/base_resource.tres",
        "base",
        "id = &\"wrong\"",
    );
    let configured = project(
        root.path(),
        "[rules]\n'architecture/resource-id-name'='warn'\n",
    );
    assert!(
        analyze_project(&configured, &[])
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.rule == "architecture/resource-id-name" && d.severity == "warning")
    );
    let configured = project(
        root.path(),
        "[rules]\n'architecture/resource-id-name'='error'\n[[overrides]]\nincludes=['src/**']\n[overrides.rules]\n'architecture/resource-id-name'='off'\n",
    );
    assert!(
        !analyze_project(&configured, &[])
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.rule == "architecture/resource-id-name")
    );
    let configured = project(
        root.path(),
        "[rules]\n'architecture/resource-id-name'='error'\n[[exceptions]]\nrule='architecture/resource-id-name'\npath='src/features/content/resources/base_resource.tres'\nitem='id'\nreason='External immutable identity'\n",
    );
    assert!(
        !analyze_project(&configured, &[])
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.rule == "architecture/resource-id-name")
    );
}

#[test]
fn dictionaries_matches_shadowing_and_resource_containers_have_precise_contexts() {
    let root = tempfile::tempdir().unwrap();
    let path = "src/features/content/resources/base_resource.gd";
    write(
        root.path(),
        path,
        &format!(
            "{BASE}@export var names: Array[StringName] = []\nfunc check(key: StringName) -> void:\n\tvar lookup: Dictionary[StringName, StringName] = {{\"key\": \"value\"}}\n\tlookup[\"index\"] = &\"ok\"\n\tlookup.has(\"query\")\n\tmatch key:\n\t\t\"pattern\":\n\t\t\tpass\nfunc other(key: String) -> void:\n\tkey = \"ordinary\"\n"
        ),
    );
    resource(
        root.path(),
        "src/features/content/resources/base_resource.tres",
        "base",
        "id = &\"base\"\nnames = Array[StringName]([\"item\"])\n",
    );
    let configured = project(root.path(), CONFIG);
    let report = analyze_project(&configured, &[]).unwrap();
    let literals: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.rule == "architecture/stringname-literal")
        .collect();
    assert_eq!(literals.len(), 6, "{literals:?}");
    assert!(!literals.iter().any(|d| d.actual == "\"ordinary\""));
}

#[test]
fn directives_inside_strings_are_not_comments_and_engine_resources_inherit_policy() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "src/features/content/resources/base_resource.gd",
        "class_name BaseResource\nextends StyleBoxFlat\n@export var id: String = \"base\"\nvar prose: String = \"\"\"\n# llm-lint-ignore architecture/resource-id-type: This is text.\n\"\"\"\n",
    );
    let configured = project(root.path(), CONFIG);
    let report = analyze_project(&configured, &[]).unwrap();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.rule == "architecture/resource-id-type")
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.rule == "architecture/comment-suppression")
    );
}

#[test]
fn relative_inheritance_constant_defaults_multiline_values_and_inner_methods_are_checked() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "src/features/content/resources/base_resource.gd",
        "class_name BaseResource\nextends Resource\nconst DEFAULT_ID: StringName = &\"base\"\n@export var id: StringName = DEFAULT_ID\n@export var names: Array[StringName] = []\nclass Inner:\n\tfunc get_name() -> StringName:\n\t\treturn \"inner\"\n",
    );
    write(
        root.path(),
        "src/features/content/resources/child_resource.gd",
        "class_name ChildResource\nextends \"base_resource.gd\"\n",
    );
    resource(
        root.path(),
        "src/features/content/resources/base_resource.tres",
        "child",
        "names = Array[StringName]([\n\t\"missing\",\n\t&\"present\"\n])",
    );
    let configured = project(root.path(), CONFIG);
    let report = analyze_project(&configured, &[]).unwrap();
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.rule.contains("resource-id")),
        "{:?}",
        report.diagnostics
    );
    let literals: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.rule == "architecture/stringname-literal")
        .collect();
    assert_eq!(literals.len(), 2, "{literals:?}");
    assert!(
        literals
            .iter()
            .any(|d| d.path.ends_with(".tres") && d.line == 6 && d.column == 2)
    );
    write(
        root.path(),
        "src/features/content/resources/base_resource.gd",
        "class_name BaseResource\nextends Resource\n@export var id := 123\n",
    );
    assert!(
        !analyze_project(&configured, &[])
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.rule.contains("resource-id"))
    );
}
