#![cfg(test)]
use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn write(root: &Path, name: &str, content: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn project(root: &Path, config: &str) {
    write(
        root,
        "llm-lint.godot.toml",
        &format!("version = 1\n{config}"),
    );
    write(root, "project.godot", "config_version=5\n");
    write(root, "src/empty.gd", "extends Node\n");
}

fn cli(root: &Path, args: &[&str]) -> (i32, String) {
    let result = Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"))
        .current_dir(root)
        .arg("references")
        .args(args)
        .output()
        .unwrap();
    let stdout = String::from_utf8(result.stdout).unwrap();
    if result.status.success() {
        assert!(
            result.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    (result.status.code().unwrap(), stdout)
}

fn query(root: &Path, subject: &str) -> Value {
    let (status, stdout) = cli(root, &[subject, "--json"]);
    assert_eq!(status, 0, "{stdout}");
    serde_json::from_str(&stdout).unwrap()
}

fn edges<'a>(report: &'a Value, section: &str) -> &'a [Value] {
    report[section].as_array().unwrap()
}

fn locations(report: &Value, section: &str, kind: &str) -> Vec<(u64, u64)> {
    edges(report, section)
        .iter()
        .filter(|e| e["type"] == kind)
        .map(|e| {
            (
                e["location"]["line"].as_u64().unwrap(),
                e["location"]["column"].as_u64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn resolves_path_aliases_and_reports_each_gdscript_occurrence() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    project(root, "");
    write(
        root,
        "src/base.gd",
        "class_name ReferenceBase\nextends Node\n",
    );
    write(root, "src/base.gd.uid", "uid://base\n");
    write(
        root,
        "src/user.gd",
        r#"extends ReferenceBase
const RESOURCE = preload(
    "res://src/base.gd"
)
var alternate: ReferenceBase
func make() -> ReferenceBase:
    var parent = ReferenceBase.new()
    print(load("uid://base"), load(("uid://base")))
    return parent
func shadow(ReferenceBase):
    print(ReferenceBase, self.ReferenceBase)
func cast(item):
    print(item as ReferenceBase, item is ReferenceBase)
"#,
    );
    write(root, "src/relative.gd", "extends \"base.gd\"\n");
    let first = query(root, "src/base.gd");
    for alias in [
        "res://src/base.gd",
        "uid://base",
        "src\\base.gd",
        root.join("src/base.gd").to_str().unwrap(),
    ] {
        assert_eq!(query(root, alias), first, "{alias}");
    }
    assert_eq!(first["version"], 1);
    assert_eq!(first["subject"]["owner"], "src");
    assert_eq!(first["subject"]["uids"], serde_json::json!(["uid://base"]));
    assert_eq!(locations(&first, "incoming", "extends"), [(1, 9), (1, 9)]);
    assert_eq!(locations(&first, "incoming", "preload"), [(2, 18)]);
    assert_eq!(
        locations(&first, "incoming", "global_class"),
        [(5, 16), (6, 16), (7, 18), (13, 19), (13, 42)]
    );
    assert_eq!(locations(&first, "incoming", "load"), [(8, 11), (8, 31)]);
    assert_eq!(edges(&first, "incoming").len(), 10);
    assert!(edges(&first, "outgoing").is_empty());
    assert!(!root.join("gdstyle.toml").exists());
    assert!(!root.join(".godot").exists());
    assert!(!root.join(".tools").exists());
}

#[test]
fn distinguishes_serialized_identity_aliases_and_saved_path_conflicts() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    project(root, "");
    write(
        root,
        "project.godot",
        r#"config_version=5
[application]
run/main_scene="uid://scene"
[autoload]
PlayerState="*uid://scene"
[editor_plugins]
enabled=PackedStringArray(
    "res://addons/p/plugin.cfg"
)
"#,
    );
    write(
        root,
        "addons/p/plugin.cfg",
        "[plugin]\nscript = \"my_plugin.gd\"\n",
    );
    write(root, "addons/p/my_plugin.gd", "extends EditorPlugin\n");
    write(root, "src/target.gd", "extends Node\n");
    write(root, "src/target.gd.uid", "uid://target\n");
    write(
        root,
        "src/uses_autoload.gd",
        "extends Node\nfunc test():\n    print(PlayerState)\nfunc shadow(PlayerState):\n    print(PlayerState)\n",
    );
    write(
        root,
        "src/scene.tscn",
        r#"[gd_scene format=3 uid="uid://scene"]
[ext_resource type="Script" uid="uid://target" path="res://src/target.gd" id="1"]
[ext_resource type="Script" uid="uid://target" path="res://src/empty.gd" id="2"]
[ext_resource type="Script" uid="uid://old" path="res://src/empty.gd" id="3"]
[ext_resource type="Script" uid="uid://missing" path="res://src/missing.gd" id="4"]
[node name="Test" type="Node"]
script = ExtResource("1")
"#,
    );
    let report = query(root, "uid://scene");
    assert_eq!(locations(&report, "incoming", "autoload_use"), [(3, 11)]);
    assert_eq!(locations(&report, "incoming", "project_setting"), [(3, 17)]);
    assert_eq!(
        locations(&report, "incoming", "autoload_registration"),
        [(5, 14)]
    );
    assert_eq!(
        locations(&report, "outgoing", "external_resource"),
        [(2, 34), (3, 34), (4, 51)]
    );
    let outgoing = edges(&report, "outgoing");
    assert_eq!(
        outgoing[0]["aliases"],
        serde_json::json!(["res://src/target.gd"])
    );
    assert_eq!(outgoing[2]["type"], "external_resource_saved_path");
    assert_eq!(outgoing[3]["aliases"], serde_json::json!(["uid://old"]));
    assert_eq!(edges(&report, "unresolved")[0]["target"], "src/missing.gd");
    let plugin = query(root, "addons/p/plugin.cfg");
    assert_eq!(
        locations(&plugin, "incoming", "plugin_registration"),
        [(8, 6)]
    );
    assert_eq!(locations(&plugin, "outgoing", "plugin_script"), [(2, 11)]);
    assert_eq!(plugin["outgoing"][0]["target"], "addons/p/my_plugin.gd");
}

#[test]
fn filters_possible_loads_and_directory_candidates() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    project(
        root,
        r#"[[dynamic_loads]]
path = "src/loader.gd"
item = "choose"
roots = ["res://src/art", "user://packs"]
extensions = ["tscn"]
reason = "Scene directory"
"#,
    );
    write(root, "src/art/flower.tscn", "[gd_scene format=3]\n");
    write(root, "src/art/flower.png", "");
    write(root, "src/art_old/flower.tscn", "[gd_scene format=3]\n");
    write(
        root,
        "src/loader.gd",
        r#"extends Node
func choose(path: String):
    return load(
        "res://src/art/" + path
    )
func scan():
    return DirAccess.get_files_at("res://src/art")
func unknown(path: String):
    return load(path)
func missing():
    return load("res://src/missing.tres")
"#,
    );
    let scene = query(root, "src/art/flower.tscn");
    assert!(edges(&scene, "incoming").is_empty());
    assert_eq!(locations(&scene, "possible", "load"), [(3, 12)]);
    assert_eq!(edges(&scene, "possible").len(), 2);
    let png = query(root, "src/art/flower.png");
    assert_eq!(edges(&png, "possible").len(), 1);
    assert!(edges(&query(root, "src/art_old/flower.tscn"), "possible").is_empty());
    let loader = query(root, "src/loader.gd");
    assert_eq!(
        loader["possible"][0]["roots"],
        serde_json::json!(["res://src/art", "user://packs"])
    );
    assert_eq!(
        loader["possible"][0]["extensions"],
        serde_json::json!(["tscn"])
    );
    assert_eq!(loader["possible"][0]["direction"], "outgoing");
    assert_eq!(loader["coverage_gaps"][0]["location"]["line"], 9);
    assert_eq!(loader["unresolved"][0]["written"], "res://src/missing.tres");
}

#[test]
fn expands_living_doc_globs_in_json_and_groups_rows_in_terminal() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    project(root, "");
    write(root, "src/area/one.gd", "extends Node\n");
    write(root, "src/area/two.gd", "extends Node\n");
    let text = r#"# Reference

## Key Files

| Path | Purpose |
| --- | --- |
| `src/area/*.gd` | Area scripts |
| `src/empty.gd` | Exact source |
| `src/missing/*.tres` | Removed resources |
"#;
    let doc = "src/docs/references/area_reference.md";
    write(root, doc, text);
    write(root, "src/docs/decisions/area_decision.md", text);
    let report = query(root, doc);
    assert_eq!(edges(&report, "outgoing").len(), 3);
    assert_eq!(report["outgoing"][0]["target"], "src/area/one.gd");
    assert_eq!(report["outgoing"][1]["target"], "src/area/two.gd");
    assert_eq!(report["outgoing"][1]["location"]["line"], 7);
    assert_eq!(report["unresolved"][0]["written"], "src/missing/*.tres");
    let source = query(root, "src/area/one.gd");
    assert_eq!(edges(&source, "incoming").len(), 1);
    assert_eq!(source["incoming"][0]["source"], doc);
    assert_eq!(source["incoming"][0]["written"], "src/area/*.gd");
    assert_eq!(
        query(root, "src/empty.gd")["incoming"][0]["type"],
        "key_files"
    );
    let (status, terminal) = cli(root, &[doc]);
    assert_eq!(status, 0);
    assert!(terminal.contains("src/area/*.gd (2 matches)"), "{terminal}");
    assert!(!terminal.contains("src/area/one.gd"), "{terminal}");
}

#[test]
fn handles_empty_files_errors_and_discovery_boundaries() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    project(
        root,
        r#"[[excluded]]
path = "vendor"
reason = "Vendor sources"
[[excluded]]
path = "scratch"
skip_discovery = true
reason = "Scratch copies"
"#,
    );
    write(
        root,
        "vendor/asset.gd",
        "class_name VendorAsset\nextends Node\n",
    );
    write(root, "vendor/asset.gd.uid", "uid://vendor\n");
    write(
        root,
        "src/use_vendor.gd",
        "extends Node\nvar example: VendorAsset\n",
    );
    write(root, "scratch/asset.gd", "extends Node\n");
    write(root, "scratch/asset.gd.uid", "uid://vendor\n");
    write(root, "src/malformed.gd", "extends Node\nfunc broken(?!\n");
    let report = query(root, "uid://vendor");
    assert_eq!(report["incoming"][0]["type"], "global_class");
    assert_eq!(report["coverage_gaps"][0]["source"], "src/malformed.gd");
    assert_eq!(report["coverage_gaps"][0]["type"], "script_syntax");
    write(root, "src/opaque.res", "RSRC");
    let opaque = query(root, "src/opaque.res");
    assert!(
        edges(&opaque, "coverage_gaps")
            .iter()
            .any(|entry| entry["type"] == "binary_resource")
    );
    let empty = query(root, "src/empty.gd");
    for section in ["incoming", "outgoing", "possible", "unresolved"] {
        assert!(edges(&empty, section).is_empty());
    }
    for invalid in [
        "src",
        "uid://missing",
        "res://../outside",
        "scratch/asset.gd",
        "../outside.gd",
    ] {
        assert_eq!(cli(root, &[invalid, "--json"]).0, 2, "{invalid}");
    }
    for args in [
        &["--json"][..],
        &["src/empty.gd", "--json", "--json"],
        &["src/empty.gd", "src/use_vendor.gd"],
    ] {
        assert_eq!(cli(root, args).0, 2, "{args:?}");
    }
    assert_eq!(
        cli(
            &root.join("src"),
            &[
                "--config",
                "../llm-lint.godot.toml",
                "--json",
                "src/empty.gd"
            ]
        )
        .0,
        0
    );
    write(root, "src/empty.gd.uid", "uid://vendor\n");
    let (status, output) = cli(root, &["src/empty.gd", "--json"]);
    assert_eq!(status, 2);
    assert!(output.contains("Duplicate UID uid://vendor"), "{output}");
}
