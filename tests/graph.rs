#![cfg(test)]
#[path = "support/rule_examples.rs"]
mod rule_examples;
use rule_examples::analyze_project;
mod support;
use support::{project, write};

#[test]
fn uid_sidecars_embedded_ids_and_unknown_ids_are_validated_without_cache() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        "src/features/content/resources/world_resource.gd",
        "class_name WorldResource\nextends Resource\n",
    );
    write(
        root.path(),
        "src/features/content/resources/world_resource.gd.uid",
        "uid://script1\n",
    );
    write(
        root.path(),
        "src/features/content/resources/world_resource.tres",
        "[gd_resource format=3 uid=\"uid://world1\"]\n[ext_resource type=\"Script\" path=\"uid://script1\" id=\"1\"]\n",
    );
    write(
        root.path(),
        "src/app/scenes/main_scene.tscn",
        "[gd_scene format=3]\n[ext_resource type=\"Resource\" path=\"uid://world1\" id=\"1\"]\n",
    );
    assert_eq!(analyze_project(&project, &[]).unwrap().errors, 0);
    write(
        root.path(),
        "src/app/scenes/main_scene.tscn",
        "[gd_scene format=3]\n[ext_resource path=\"uid://missing\"]\n",
    );
    assert!(
        analyze_project(&project, &[])
            .unwrap()
            .counts
            .contains_key("architecture/reference")
    );
    write(
        root.path(),
        "src/features/content/resources/other_resource.tres",
        "[gd_resource format=3 uid=\"uid://world1\"]\n",
    );
    assert!(analyze_project(&project, &[]).is_err());
}

#[test]
fn scratch_exclusions_prune_only_the_configured_path_before_indexing() {
    let root = tempfile::tempdir().unwrap();
    let project = project(
        root.path(),
        "[[excluded]]\npath='local/scratch'\nreason='Local project copies'\noptional=true\nskip_discovery=true\n",
    );
    let script = "class_name RunModel\nextends RefCounted\n";
    let uid = "uid://runmodel\n";
    let source = "src/features/run/models/run_model.gd";
    write(root.path(), source, script);
    write(root.path(), &format!("{source}.uid"), uid);
    let original = analyze_project(&project, &[]).unwrap();
    assert_eq!(original.errors, 0);

    write(root.path(), "local/scratch/copied/run_model.gd", script);
    write(root.path(), "local/scratch/copied/run_model.gd.uid", uid);
    write(root.path(), "local/scratch/copied/README.md", "scratch");
    let populated = analyze_project(&project, &[]).unwrap();
    assert_eq!(populated.errors, 0);
    assert_eq!(populated.scanned_files, original.scanned_files);
    let corpus = godot_llm_lint::features::architecture::helpers::corpus(&project).unwrap();
    assert!(!corpus.paths.contains("local/scratch"));
    assert_eq!(corpus.uids["uid://runmodel"], source);

    let sibling = "local/scratch-old/run_model.gd";
    write(root.path(), sibling, script);
    write(root.path(), &format!("{sibling}.uid"), uid);
    let error = analyze_project(&project, &[]).unwrap_err().to_string();
    assert!(error.contains("Duplicate UID uid://runmodel"), "{error}");
    assert!(error.contains(source) && error.contains(sibling), "{error}");
}

#[test]
fn plugin_registration_autoload_aliases_and_editor_runtime_boundaries_are_enforced() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "addons/editor/plugin.gd",
        "extends EditorPlugin\n",
    );
    std::fs::create_dir_all(root.path().join("tools")).unwrap();
    let project = project(
        root.path(),
        "plugin_roots=['addons/editor']\ntooling_roots=['tools']\n[[entrypoints]]\npath='addons/editor/plugin.gd'\nreason='Native plugin entrypoint'\n",
    );
    write(
        root.path(),
        "project.godot",
        "config_version=5\n[autoload]\nGlobals=\"*res://src/app/nodes/globals_node.gd\"\n[editor_plugins]\nenabled=PackedStringArray(\"res://addons/editor/plugin.cfg\")\n",
    );
    write(
        root.path(),
        "addons/editor/plugin.cfg",
        "[plugin]\nscript=\"plugin.gd\"\n",
    );
    write(
        root.path(),
        "src/app/nodes/globals_node.gd",
        "extends Node\n",
    );
    write(
        root.path(),
        "src/features/run/models/run_model.gd",
        "extends RefCounted\nfunc get_global() -> Node:\n\treturn Globals\n",
    );
    write(
        root.path(),
        "tools/queries/read_query.gd",
        "extends RefCounted\nstatic func read_value() -> int:\n\treturn 1\n",
    );
    let report = analyze_project(&project, &[]).unwrap();
    assert!(
        report
            .counts
            .contains_key("architecture/dependency-direction")
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path == "project.godot")
    );
    write(
        root.path(),
        "src/app/nodes/globals_node.gd",
        "class_name Globals\nextends Node\n",
    );
    assert!(
        analyze_project(&project, &[])
            .unwrap()
            .counts
            .contains_key("architecture/class-unique")
    );
    write(
        root.path(),
        "addons/editor/nodes/panel_node.gd",
        "extends Control\nconst MAIN=preload('res://src/app/nodes/globals_node.gd')\n",
    );
    assert!(
        analyze_project(&project, &[])
            .unwrap()
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path.contains("panel_node")
                && diagnostic.rule == "architecture/dependency-direction")
    );
}

#[test]
fn vendor_globals_remain_visible_but_vendor_implementation_is_not_linted() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "addons/vendor/test.gd",
        "class_name VendorTest\nextends Node\nfunc badlyNamed() -> void:\n\tpass\n",
    );
    let project = project(
        root.path(),
        "[[excluded]]\npath='addons/vendor'\nreason='Upstream test framework'\n",
    );
    write(
        root.path(),
        "src/features/run/testing/test_run.gd",
        "extends VendorTest\nfunc test_run() -> void:\n\tpass\n",
    );
    assert_eq!(analyze_project(&project, &[]).unwrap().errors, 0);
    write(
        root.path(),
        "src/features/run/models/run_model.gd",
        "class_name VendorTest\nextends RefCounted\n",
    );
    assert!(
        analyze_project(&project, &[])
            .unwrap()
            .counts
            .contains_key("architecture/class-unique")
    );
}

#[test]
fn dynamic_roots_follow_every_bundled_dependency_and_stale_loads_fail() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "src/shared/queries/load_query.gd",
        "extends RefCounted\nstatic func load_value(path: String) -> Resource:\n\treturn load(path)\n",
    );
    write(
        root.path(),
        "src/features/content/resources/world_resource.tres",
        "[gd_resource format=3]\n",
    );
    let config = "[[dynamic_loads]]\npath='src/shared/queries/load_query.gd'\nitem='load_value'\nroots=['res://src/features/content/resources']\nreason='Explicit nonportable fixture'\n";
    let project = project(root.path(), config);
    assert!(
        analyze_project(&project, &[])
            .unwrap()
            .counts
            .contains_key("architecture/dependency-direction")
    );
    write(
        root.path(),
        "src/features/content/resources/world_resource.tres",
        "[gd_resource format=3]\n[ext_resource path=\"res://src/app/testing/test_helper.gd\"]\n",
    );
    write(
        root.path(),
        "src/app/testing/test_helper.gd",
        "extends RefCounted\n",
    );
    assert!(
        analyze_project(&project, &[])
            .unwrap()
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path.ends_with("world_resource.tres"))
    );
    let project = support::project(root.path(), "");
    assert!(
        analyze_project(&project, &[])
            .unwrap()
            .counts
            .contains_key("architecture/dynamic-load")
    );
}

#[test]
fn dynamic_extension_filters_preserve_nested_testing_dependency_checks() {
    let root = tempfile::tempdir().unwrap();
    let load = "src/shared/subject/value_query.gd";
    let test_dir = "src/shared/subject/testing";
    write(
        root.path(),
        load,
        "extends RefCounted\nstatic func load_value(path: String) -> Resource:\n\treturn load(path)\n",
    );
    write(
        root.path(),
        &format!("{test_dir}/test_value.gd"),
        "extends RefCounted\n",
    );
    let config = format!(
        "[bundles]\nowners=['src/shared']\n[[dynamic_loads]]\npath='{load}'\nitem='load_value'\nroots=['res://src/shared/subject']\nextensions=['tres']\nreason='Only authored text resources can be selected.'\n"
    );
    let project = project(root.path(), &config);
    assert!(
        !analyze_project(&project, &[])
            .unwrap()
            .counts
            .contains_key("architecture/dependency-direction")
    );
    let fixture = format!("{test_dir}/test_value_resource.tres");
    write(root.path(), &fixture, "[gd_resource format=3]\n");
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(report.counts["architecture/dependency-direction"], 1);
    assert!(report.diagnostics.iter().any(|item| item.actual == fixture));
}

#[test]
fn role_suffixes_structural_bases_and_reasoned_suppressions_are_checked() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        "src/features/run/resources/run_resource.gd",
        "extends RefCounted\n",
    );
    write(
        root.path(),
        "src/features/run/models/run_model.gd",
        "extends Node\n",
    );
    write(
        root.path(),
        "src/features/run/queries/run.gd",
        "extends RefCounted\nstatic func get_run() -> int:\n\treturn 1\n",
    );
    write(
        root.path(),
        "src/features/run/testing/Run.gd",
        "extends Node\n",
    );
    write(root.path(), "other.gd", "extends Node\n");
    let report = analyze_project(&project, &[]).unwrap();
    for rule in [
        "architecture/role-base",
        "architecture/role-filename",
        "architecture/file-name",
        "architecture/feature-layout",
        "architecture/role-placement",
    ] {
        assert!(report.counts.contains_key(rule), "{rule}");
    }
    write(
        root.path(),
        "src/shared/queries/run_query.gd",
        "# gdstyle:ignore-file\nextends RefCounted\n",
    );
    assert!(
        analyze_project(&project, &[])
            .unwrap()
            .counts
            .contains_key("architecture/native-suppression")
    );
    write(
        root.path(),
        "src/shared/queries/run_query.gd",
        "# gdstyle:ignore=quality/type-hint # reason: Foreign callback contract\nextends RefCounted\n",
    );
    assert!(
        !analyze_project(&project, &[])
            .unwrap()
            .counts
            .contains_key("architecture/native-suppression")
    );
}

#[test]
fn native_suppressions_accept_adjacent_reasons_without_weakening_precision() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    let cases = [
        (
            "# reason: Separate mutable instances\n# gdstyle:ignore=quality/allocation-in-loop\nextends RefCounted\n",
            false,
        ),
        (
            "# gdstyle:ignore=quality/max-class-variables\n# reason: Exported serialization contract\nextends RefCounted\n",
            false,
        ),
        (
            "# reason: Contract\n# gdstyle:ignore=quality/type-hint,quality/max-parameters\nextends RefCounted\n",
            true,
        ),
        (
            "# reason: Contract\n# gdstyle:ignore-file=quality/max-class-variables\nextends RefCounted\n",
            true,
        ),
        (
            "# reason:\n# gdstyle:ignore=quality/type-hint\nextends RefCounted\n",
            true,
        ),
        (
            "# reason: Too far away\n\n# gdstyle:ignore=quality/type-hint\nextends RefCounted\n",
            true,
        ),
    ];
    for (source, rejected) in cases {
        write(root.path(), "src/shared/helpers/value_helpers.gd", source);
        let report = analyze_project(&project, &[]).unwrap();
        assert_eq!(
            report
                .counts
                .contains_key("architecture/native-suppression"),
            rejected,
            "{source}"
        );
    }
}

#[test]
fn directory_literals_and_private_static_helpers_preserve_native_layouts() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        "src/features/content/resources/world_resource.gd",
        "extends Resource\n",
    );
    write(
        root.path(),
        "src/shared/helpers/data_helpers.gd",
        "extends RefCounted\nconst ROOT='res://src/features/content/resources/'\nstatic func value() -> int:\n\treturn _private()\nstatic func _private() -> int:\n\treturn 1\n",
    );
    assert!(
        analyze_project(&project, &[])
            .unwrap()
            .counts
            .contains_key("architecture/dependency-direction")
    );
    write(
        root.path(),
        "src/features/content/resources/world_resource.gd.uid",
        "not-a-uid\n",
    );
    assert!(
        analyze_project(&project, &["src/features/content".into()])
            .unwrap()
            .errors
            == 0
    );
}

#[test]
fn imported_asset_uids_keep_feature_ownership_and_local_inheritance_is_native() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        "src/features/content/resources/base_resource.gd",
        "class_name BaseResource\nextends Resource\n",
    );
    write(
        root.path(),
        "src/features/content/resources/child_resource.gd",
        "class_name ChildResource\nextends BaseResource\n",
    );
    write(
        root.path(),
        "src/features/content/resources/literal_resource.gd",
        "extends 'res://src/features/content/resources/base_resource.gd'\n",
    );
    write(
        root.path(),
        "src/features/content/assets/icon.svg",
        "<svg/>\n",
    );
    write(
        root.path(),
        "src/features/content/assets/icon.svg.import",
        "[remap]\nuid=\"uid://icon\"\n",
    );
    write(
        root.path(),
        "src/shared/helpers/icon_helpers.gd",
        "extends RefCounted\nconst ICON=preload('uid://icon')\n",
    );
    let report = analyze_project(&project, &[]).unwrap();
    assert!(!report.counts.contains_key("architecture/role-base"));
    assert!(
        report
            .counts
            .contains_key("architecture/dependency-direction")
    );
}
