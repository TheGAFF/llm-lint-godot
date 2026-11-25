#![cfg(test)]
mod support;

use godot_llm_lint::{
    features::architecture::helpers::analyze_project, shared::models::Diagnostic,
};
use support::{project, write};

const SUBJECT: &str = "src/features/gameplay/nodes/subject_node.gd";
const CALLER: &str = "src/features/ui/nodes/caller_node.gd";
const REQUIRED: &str = "architecture/member-underscore-required";
const ACCESS: &str = "architecture/private-member-access";
const LOCAL: &str = "architecture/local-variable-snake-case";

fn items<'a>(diagnostics: &'a [Diagnostic], rule: &str) -> Vec<&'a str> {
    diagnostics
        .iter()
        .filter(|item| item.rule == rule)
        .map(|item| item.item.as_str())
        .collect()
}

#[test]
fn names_use_declaration_identity_and_all_callers_even_in_a_targeted_run() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        SUBJECT,
        r#"class_name SubjectNode
extends Node

var own_value: int
var public_value: int
var unused_value: int
var shadowed: int
var possible: int

func local_method() -> void:
	self.own_value = public_value
	possible = 1

func public_method() -> void:
	pass

func _ready() -> void:
	local_method()
	public_method()
	var shadowed: int = 1
	print(shadowed)
	print("unused_value")
"#,
    );
    write(
        root.path(),
        CALLER,
        r#"class_name CallerNode
extends Node

var own_value: int

func refresh(subject: SubjectNode, maybe: Variant) -> void:
	subject.public_method()
	subject.public_value = 3
	maybe.possible = 1
	maybe._unknown()
	# subject.local_method()
	print("subject.own_value")
"#,
    );
    let report = analyze_project(&project, &[SUBJECT.into()]).unwrap();
    assert_eq!(
        items(&report.diagnostics, REQUIRED),
        ["own_value", "local_method"]
    );
    assert!(items(&report.diagnostics, ACCESS).is_empty());
    let own_value = report
        .diagnostics
        .iter()
        .find(|item| item.rule == REQUIRED)
        .unwrap();
    assert_eq!(
        (own_value.path.as_str(), own_value.line, own_value.column),
        (SUBJECT, 4, 5)
    );
}

#[test]
fn private_calls_callable_references_reads_and_writes_resolve_through_types() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        SUBJECT,
        r#"class_name SubjectNode
extends Node

class Nested:
	static func _nested() -> void:
		pass

var _value: int

static func _static_method() -> void:
	pass

func _private_method() -> void:
	pass
"#,
    );
    write(
        root.path(),
        CALLER,
        r#"class_name CallerNode
extends Node

const Subject = preload("res://src/features/gameplay/nodes/subject_node.gd")
var subject: Subject

func _make() -> SubjectNode:
	return Subject.new()

func refresh(dynamic: Variant) -> void:
	Subject._static_method()
	SubjectNode.Nested._nested()
	var target := Subject.new()
	target._value = 2
	print(subject._value)
	var callback := target._private_method
	_make()._private_method()
	(dynamic as SubjectNode)._private_method()
	Subject.new()._private_method()
	target.call_deferred(&"_private_method")
	target.callv("_private_method", [])
	target.call("_private_method")
	Callable(target, &"_private_method")
	target.set(&"_value", 1)
	target.get("_value")
	target["_value"] = 2
	var dictionary := {}
	dictionary.get("_value")
	dynamic._private_method()
	print("_private_method")
"#,
    );
    let report = analyze_project(&project, &[CALLER.into()]).unwrap();
    let accesses: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|item| item.rule == ACCESS)
        .collect();
    assert_eq!(
        accesses.iter().map(|item| item.line).collect::<Vec<_>>(),
        (11..=26).filter(|line| *line != 13).collect::<Vec<_>>(),
        "{accesses:?}"
    );
    assert!(
        accesses
            .iter()
            .all(|item| item.path == CALLER && item.actual.contains(SUBJECT))
    );
}

#[test]
fn inherited_hooks_are_allowed_only_for_base_calls() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        SUBJECT,
        r#"class_name SubjectNode
extends Node2D

func _ready() -> void:
	public_hook()

func _extension() -> void:
	pass

func _private_helper() -> void:
	pass

func public_hook() -> void:
	pass
"#,
    );
    write(
        root.path(),
        CALLER,
        r#"class_name CallerNode
extends "res://src/features/gameplay/nodes/subject_node.gd"

func _extension() -> void:
	super()
	super._extension()

func public_hook() -> void:
	pass

func call_parent() -> void:
	super._ready()
	super._private_helper()

func from_other(subject: SubjectNode) -> void:
	subject._ready()
	subject._extension()
"#,
    );
    let report = analyze_project(&project, &[]).unwrap();
    assert!(items(&report.diagnostics, REQUIRED).is_empty());
    assert_eq!(
        items(&report.diagnostics, ACCESS),
        ["_private_helper", "_ready", "_extension"]
    );
}

#[test]
fn godot_exposure_and_excluded_scripts_provide_external_evidence() {
    let root = tempfile::tempdir().unwrap();
    let project = project(
        root.path(),
        r#"
[[excluded]]
path = "addons/vendor"
reason = "Vendor fixture"
optional = true
"#,
    );
    write(
        root.path(),
        "project.godot",
        r#"config_version=5
[autoload]
Spawner="*res://src/features/gameplay/nodes/subject_node.gd"
"#,
    );
    write(
        root.path(),
        SUBJECT,
        r#"class_name SubjectNode
extends Node

@export var property: int

@rpc("any_peer")
func rpc_handler() -> void:
	pass

func on_pressed() -> void:
	pass

func _on_private_pressed() -> void:
	pass

func vendor_api() -> void:
	pass

func _ready() -> void:
	property = 1
	rpc_handler()
	on_pressed()
	vendor_api()
"#,
    );
    write(
        root.path(),
        "src/features/gameplay/scenes/subject_scene.tscn",
        r#"[gd_scene load_steps=2 format=3]
[ext_resource type="Script" path="res://src/features/gameplay/nodes/subject_node.gd" id="1"]
[node name="Subject" type="Node"]
script = ExtResource("1")
"#,
    );
    write(
        root.path(),
        "src/features/ui/scenes/caller_scene.tscn",
        r#"[gd_scene load_steps=2 format=3]
[ext_resource type="PackedScene" path="res://src/features/gameplay/scenes/subject_scene.tscn" id="1"]
[node name="Caller" type="Node"]
[node name="Subject" parent="." instance=ExtResource("1")]
[connection signal="ready" from="." to="Subject" method="on_pressed"]
[connection signal="ready" from="." to="Subject" method="_on_private_pressed"]
"#,
    );
    write(
        root.path(),
        "addons/vendor/caller.gd",
        r#"extends Node
func _ready() -> void:
	var _ignored = Spawner
	_ignored.vendor_api()
	_ignored._on_private_pressed()
"#,
    );
    write(
        root.path(),
        CALLER,
        r#"class_name CallerNode
extends Node

func _ready() -> void:
	Spawner._on_private_pressed()
"#,
    );
    for targets in [vec![], vec![SUBJECT.into(), CALLER.into()]] {
        let report = analyze_project(&project, &targets).unwrap();
        assert!(
            items(&report.diagnostics, REQUIRED).is_empty(),
            "{:?}",
            report.diagnostics
        );
        assert!(items(&report.diagnostics, LOCAL).is_empty());
        assert_eq!(items(&report.diagnostics, ACCESS), ["_on_private_pressed"]);
    }
}

#[test]
fn locals_include_loops_patterns_and_lambdas_and_honor_configuration() {
    let root = tempfile::tempdir().unwrap();
    let project = project(
        root.path(),
        r#"
[[overrides]]
includes = ["src/features/gameplay/**"]
[overrides.rules]
"architecture/local-variable-snake-case" = "warn"
"#,
    );
    write(
        root.path(),
        SUBJECT,
        r#"class_name SubjectNode
extends Node

func _visit(_arg: int) -> void:
	const CONSTANT := 3
	var good_name := CONSTANT
	var mixedCase := good_name
	# llm-lint-ignore architecture/local-variable-snake-case: Foreign fixture name.
	var _suppressed := 0
	for _index in 3:
		pass
	match good_name:
		var _bound:
			pass
		_:
			pass
	var callback := func():
		var _nested := 1
"#,
    );
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(
        items(&report.diagnostics, LOCAL),
        ["mixedCase", "_index", "_bound", "_nested"]
    );
    assert!(
        report
            .diagnostics
            .iter()
            .filter(|item| item.rule == LOCAL)
            .all(|item| item.severity == "warning")
    );
    assert!(items(&report.diagnostics, "architecture/comment-suppression").is_empty());
}
