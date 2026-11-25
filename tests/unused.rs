#![cfg(test)]
use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn write(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn project(root: &Path, config: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    write(
        root,
        "llm-lint.godot.toml",
        &format!("version = 1\n{config}"),
    );
    write(
        root,
        "project.godot",
        "config_version=5\n[application]\nrun/main_scene=\"res://src/start.tscn\"\n",
    );
    write(
        root,
        "src/start.tscn",
        "[gd_scene format=3]\n[node name=\"Main\" type=\"Node\"]\n",
    );
}

fn cli(root: &Path, args: &[&str]) -> (i32, String, Value) {
    let result = Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"))
        .current_dir(root)
        .args(["unused", "--json"])
        .args(args)
        .output()
        .unwrap();
    let text = String::from_utf8(result.stdout).unwrap();
    let json = serde_json::from_str(&text).unwrap_or_else(|error| {
        panic!(
            "{error}: {text}\n{}",
            String::from_utf8_lossy(&result.stderr)
        )
    });
    (result.status.code().unwrap(), text, json)
}

fn values<'a>(report: &'a Value, key: &str) -> Vec<&'a str> {
    report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|finding| finding[key].as_str().unwrap())
        .collect()
}

#[test]
fn orphan_cycles_do_not_keep_images_and_uids_and_relative_edges_resolve() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    project(root, "");
    let scene = r#"[gd_scene format=3 uid="uid://main"]
[ext_resource type="Texture2D" uid="uid://used" path="old.png" id="1"]
[ext_resource type="Texture2D" uid="uid://stale" path="art/relative.webp" id="2"]
[ext_resource type="Shader" path="shaders/effect.gdshader" id="3"]
[ext_resource type="Script" path="relative.gd" id="4"]
[node name="Main" type="Node"]
script = ExtResource("4")
"#;
    write(root, "src/start.tscn", scene);
    write(
        root,
        "src/relative.gd",
        "extends \"support/base.gd\"\nfunc _ready() -> void:\n\tprint(DirAccess.get_files_at(\"res://\"))\n",
    );
    write(root, "src/support/base.gd", "extends Node\n");
    write(root, "src/art/live.png", "imported live");
    write(
        root,
        "src/art/live.png.import",
        "[remap]\nuid=\"uid://used\"\n",
    );
    write(root, "src/art/relative.webp", "relative live");
    write(
        root,
        "src/shaders/effect.gdshader",
        "#include \"fragment.gdshaderinc\"\nshader_type canvas_item;\n",
    );
    write(root, "src/shaders/fragment.gdshaderinc", "// shared");
    write(
        root,
        "src/dead_a.tres",
        "[gd_resource format=3]\n[ext_resource path=\"dead_b.tres\" id=\"1\"]\n",
    );
    write(
        root,
        "src/dead_b.tres",
        "[gd_resource format=3]\n[ext_resource path=\"dead_a.tres\" id=\"1\"]\n[ext_resource path=\"art/large.webp\" id=\"2\"]\n",
    );
    write(root, "src/art/large.webp", "the largest orphan source");
    write(
        root,
        "src/art/large.webp.import",
        "[remap]\nuid=\"uid://orphan\"\n",
    );
    write(root, "src/art/small.png", "small");

    let (code, first, report) = cli(root, &[]);
    assert_eq!(code, 0);
    assert_eq!(report["version"], 1);
    assert_eq!(report["coverage_gaps"], serde_json::json!([]));
    assert_eq!(
        values(&report, "path"),
        [
            "src/art/large.webp",
            "src/art/small.png",
            "src/dead_a.tres",
            "src/dead_b.tres",
        ]
    );
    assert_eq!(
        report["diagnostics"][0]["companions"],
        serde_json::json!(["src/art/large.webp.import"])
    );
    assert_eq!(report["diagnostics"][0]["bytes"], 25);
    assert_eq!(cli(root, &[]).1, first);
    assert_eq!(
        cli(root, &["src/art", "--kind", "asset", "--fail-on-findings"]).0,
        1
    );
    assert_eq!(cli(root, &["src/shaders", "--fail-on-findings"]).0, 0);
    assert_eq!(
        fs::read_to_string(root.join("src/start.tscn")).unwrap(),
        scene
    );
    assert!(!root.join("gdstyle.toml").exists());
    assert!(!root.join(".godot").exists());
}

#[test]
fn project_roots_authoring_inputs_dynamic_bounds_and_exclusions() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    project(root, ROOTS_CONFIG);
    authoring_tree(root);
    let (code, _, report) = cli(root, &["--kind", "asset", "--fail-on-findings"]);
    assert_eq!(code, 1, "{report}");
    assert_eq!(
        values(&report, "path"),
        [
            "src/bound-old/dead.png",
            "src/omit/no_export.png",
            "src/orphan.png",
        ]
    );
    assert_eq!(report["coverage_gaps"], serde_json::json!([]));
}

const ROOTS_CONFIG: &str = r#"
tooling_roots = ["tools"]
[[dynamic_loads]]
path = "src/testing/editor_test_harness.gd"
item = "pick"
roots = ["res://src/bound"]
reason = "Known catalog directory"
[[excluded]]
path = "vendor"
reason = "Upstream"
[[excluded]]
path = "scratch"
skip_discovery = true
reason = "Local copies"
[unused]
reference_files = ["docs/extra.json", "docs/guide.html"]
[[unused.keep]]
path = "src/keep"
reason = "External art pipeline"
"#;

fn authoring_tree(root: &Path) {
    write(
        root,
        "project.godot",
        r#"config_version=5
[application]
run/main_scene="res://src/start.tscn"
[autoload]
Global="*uid://testautoload"
[editor_plugins]
enabled=PackedStringArray(
    "res://addons/active/plugin.cfg",
)
"#,
    );
    write(root, "src/global.gd.uid", "uid://testautoload");
    write(
        root,
        "src/global.gd",
        "extends Node\nconst VEN = preload(\"res://vendor/bridge.gd\")\n",
    );
    write(
        root,
        "vendor/bridge.gd",
        "extends Node\nconst ART = preload(\"res://src/vendor_used.svg\")\n",
    );
    write(
        root,
        "addons/active/plugin.cfg",
        "[plugin]\nscript = \"plugin.gd\"\n",
    );
    write(
        root,
        "addons/active/plugin.gd",
        "extends EditorPlugin\nconst ART = preload(\"res://src/plugin.png\")\n",
    );
    write(
        root,
        "src/testing/editor_test_harness.gd",
        "extends SceneTree\nfunc pick(path: String) -> void:\n\tload(path)\n",
    );
    write(
        root,
        "tools/check.py",
        "manifest = ROOT / 'docs/revision.json'\n",
    );
    write(
        root,
        "docs/revision.json",
        r#"{"src/icon.svg": {"runtime": "src/icon.png"}}"#,
    );
    write(root, "docs/extra.json", r#"["res://src/manual.webp"]"#);
    write(
        root,
        "docs/guide.html",
        r#"<img src="../src/guide_only.png">"#,
    );
    write(root, "docs/history.json", r#"["res://src/orphan.png"]"#);
    write(
        root,
        "export_presets.cfg",
        "[preset.0]\nexport_filter=\"scenes\"\nexport_files=PackedStringArray(\n\"res://src/bonus.tscn\"\n)\ninclude_filter=\"src/*/*export.PNG\"\nexclude_filter=\"src/omit/*\"\n",
    );
    write(
        root,
        "src/bonus.tscn",
        "[gd_scene format=3]\n[ext_resource path=\"bonus.png\" id=\"1\"]\n",
    );
    for path in [
        "src/vendor_used.svg",
        "src/plugin.png",
        "src/bound/live.webp",
        "src/bound-old/dead.png",
        "src/keep/manual.png",
        "src/icon.svg",
        "src/icon.png",
        "src/manual.webp",
        "src/guide_only.png",
        "src/bonus.png",
        "src/orphan.png",
        "src/export/deep/yes_export.png",
        "src/omit/no_export.png",
        "vendor/old.png",
        "scratch/old.png",
    ] {
        write(root, path, "image");
    }
    write(root, "scratch/copy.gd", "extends Node\n");
    write(root, "scratch/copy.gd.uid", "uid://testautoload");
}

#[test]
fn symbol_audit_keeps_godot_callbacks_and_resolves_scoped_references() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    project(
        root,
        r#"
[[unused.keep]]
path = "src/main.gd"
item = "from_external"
reason = "Plugin callback"
"#,
    );
    write(
        root,
        "src/start.tscn",
        r#"[gd_scene format=3]
[ext_resource type="Script" path="res://src/main.gd" id="1"]
[sub_resource type="Animation" id="anim"]
tracks/0/type = "method"
tracks/0/keys = {"values": [{
"method": &"_animation_hook"
}]}
[node name="Main" type="Node"]
script = ExtResource("1")
[connection signal="ready" from="." to="." method="_connected"]
"#,
    );
    write(
        root,
        "src/main.gd",
        r#"extends Node

@export var editor_field: int
var shadowed: int
var active: int
var animated: int
const UNREFERENCED = 3
const REFERENCED = 5

func _ready() -> void:
	var shadowed := REFERENCED
	var orphan_local := 6
	print(shadowed)
	active = 4
	Callable(self, &"_callable").call()
	create_tween().tween_property(self, "animated", 1, 0.2)

func _connected() -> void:
	pass

func _callable() -> void:
	pass

func _animation_hook() -> void:
	pass

@rpc
func remotely_called() -> void:
	pass

func from_external() -> void:
	pass

func _orphan_function() -> void:
	pass
"#,
    );
    write(
        root,
        "src/unreachable.gd",
        "extends Node\nfunc extra() -> void:\n\t$Main._orphan_function()\n",
    );
    let (code, _, report) = cli(root, &["src/main.gd", "--fail-on-findings"]);
    assert_eq!(code, 1, "{report}");
    assert_eq!(
        values(&report, "item"),
        [
            "_orphan_function",
            "shadowed",
            "UNREFERENCED",
            "_ready.orphan_local",
        ]
    );
    assert_eq!(report["diagnostics"][3]["line"], 12);
}

#[test]
fn strict_exit_reports_coverage_gaps_for_the_selected_kind() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    project(root, "");
    write(
        root,
        "src/start.tscn",
        "[gd_scene format=3]\n[ext_resource path=\"res://src/main.gd\" id=\"1\"]\n",
    );
    write(
        root,
        "src/main.gd",
        "extends Node\nfunc _ready() -> void:\n\tload(get_meta(\"path\"))\n\tcall(get_meta(\"method\"))\n",
    );
    let (code, _, report) = cli(root, &["--kind", "asset"]);
    assert_eq!(code, 0);
    assert_eq!(report["coverage_gaps"].as_array().unwrap().len(), 1);
    assert_eq!(cli(root, &["--kind", "asset", "--fail-on-findings"]).0, 2);
    assert_eq!(
        cli(root, &["--kind", "function", "--fail-on-findings"]).0,
        2
    );
    assert_eq!(
        cli(root, &["--kind", "variable", "--fail-on-findings"]).0,
        0
    );
    assert_eq!(cli(root, &["--kind", "unknown"]).0, 2);
    let config = root.join("llm-lint.godot.toml");
    let output = Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"))
        .current_dir(root.join("src"))
        .args(["unused", "--kind", "asset", "--config"])
        .arg(config)
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn release_advisories_distinguish_runtime_uses_and_keep_manifest_reasons() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    project(
        root,
        r#"
tooling_roots = ["tools"]
[[excluded]]
path = "src/local.png"
reason = "Local artwork"
[unused]
release_exclusion_files = ["tools/release.json"]
reference_files = ["docs/history.json"]
[[unused.keep]]
path = "src/retired.png"
reason = "Original selection can be compared"
"#,
    );
    write(
        root,
        "src/start.tscn",
        r#"[gd_scene format=3]
[ext_resource path="live.png" id="1"]
[ext_resource path="res://src/release_catalog.json" id="2"]
"#,
    );
    write(
        root,
        "src/release_catalog.json",
        r#"{"texture":"res://src/live.png"}"#,
    );
    write(root, "src/retired.png", "previous menu painting");
    write(
        root,
        "src/retired.png.import",
        "[remap]\nuid=\"uid://retired\"\n",
    );
    write(root, "src/live.png", "live");
    write(root, "src/local.png", "local");
    write(
        root,
        "tools/release.json",
        r#"{"asset_exclusions":[
        {"path":"src/live.png","reason":"Verify that conflicting exclusions surface"},
        {"path":"src/retired.png","reason":"Previous menu painting"},
        {"path":"src/local.png","reason":"Local only"}
    ]}"#,
    );
    write(root, "tools/audit.py", "config = 'tools/release.json'\n");
    write(
        root,
        "docs/history.json",
        r#"{"retired":"src/retired.png"}"#,
    );

    let (status, _, result) = cli(root, &["--kind", "asset", "--fail-on-findings"]);
    assert_eq!(status, 0, "{result}");
    assert_eq!(result["diagnostics"], serde_json::json!([]));
    let rows = result["release_exclusions"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["path"], "src/retired.png");
    assert_eq!(rows[0]["reachable_from_runtime"], false);
    assert_eq!(
        rows[0]["companions"],
        serde_json::json!(["src/retired.png.import"])
    );
    assert_eq!(
        rows[0]["retained_by"],
        serde_json::json!([
            {"kind":"release_exclusion","path":"tools/release.json","reason":"Previous menu painting"},
            {"kind":"project_reference","path":"docs/history.json"},
            {"kind":"project_reference","path":"llm-lint.godot.toml","reason":"Original selection can be compared"}
        ])
    );
    assert_eq!(rows[1]["path"], "src/live.png");
    assert_eq!(rows[1]["reachable_from_runtime"], true);
    assert_eq!(
        rows[1]["retained_by"][1]["path"],
        "src/release_catalog.json"
    );
    assert_eq!(rows[1]["retained_by"][2]["path"], "src/start.tscn");
    assert_eq!(
        cli(root, &["src/live.png"]).2["release_exclusions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        cli(root, &["--kind", "file"])
            .2
            .get("release_exclusions")
            .is_none()
    );
}
