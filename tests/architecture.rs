#![cfg(test)]
#[path = "support/rule_examples.rs"]
mod rule_examples;
use rule_examples::analyze_project;
mod support;
use support::{project, write};

#[test]
fn roles_keep_stateful_methods_and_engine_callbacks_in_their_class() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        "src/features/run/models/run_model.gd",
        "class_name RunModel\nextends RefCounted\nvar _paused: bool = false\nfunc get_state() -> bool:\n\treturn _paused\nfunc _init() -> void:\n\t_paused = false\n",
    );
    write(
        root.path(),
        "src/features/run/queries/read_run_query.gd",
        "class_name ReadRunQuery\nextends RefCounted\nstatic func get_run() -> int:\n\treturn 1\n",
    );
    assert_eq!(analyze_project(&project, &[]).unwrap().errors, 0);
    write(
        root.path(),
        "src/features/run/queries/read_run_query.gd",
        "class_name Wrong\nextends Node\nstatic func validate_run() -> bool:\n\treturn true\n",
    );
    let report = analyze_project(&project, &[]).unwrap();
    assert!(report.counts.contains_key("architecture/class-name"));
    assert!(report.counts.contains_key("architecture/function-role"));
}

#[test]
fn native_acronyms_and_dimensions_match_snake_case_script_stems() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        "src/features/net/nodes/http_request_node.gd",
        "class_name HTTPRequestNode\nextends Node\n",
    );
    write(
        root.path(),
        "src/features/world/nodes/character_2d_node.gd",
        "class_name Character2DNode\nextends Node2D\n",
    );
    assert_eq!(analyze_project(&project, &[]).unwrap().errors, 0);
}

#[test]
fn graph_follows_global_classes_scene_resources_and_rejects_wrong_case() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        "src/features/run/models/run_model.gd",
        "class_name RunModel\nextends RefCounted\n",
    );
    write(
        root.path(),
        "src/shared/queries/read_run_query.gd",
        "extends RefCounted\nstatic func read_run() -> RunModel:\n\treturn RunModel.new()\n",
    );
    write(
        root.path(),
        "src/app/scenes/main_scene.tscn",
        "[gd_scene format=3]\n[ext_resource type=\"Script\" path=\"res://src/features/run/models/Run_model.gd\" id=\"1\"]\n",
    );
    let report = analyze_project(&project, &[]).unwrap();
    assert!(
        report
            .counts
            .contains_key("architecture/dependency-direction")
    );
    assert!(report.counts.contains_key("architecture/reference"));
    assert!(
        analyze_project(&project, &["src/shared/queries/read_run_query.gd".into()])
            .unwrap()
            .errors
            > 0
    );
}

#[test]
fn collections_have_xor_size_state_and_prefix_contracts() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "collection_max_code_lines = 2\n");
    write(
        root.path(),
        "src/features/run/run_queries.gd",
        "class_name RunQueries\nextends RefCounted\nstatic func get_run() -> int:\n\treturn 1\n",
    );
    write(
        root.path(),
        "src/features/run/queries/other_query.gd",
        "extends RefCounted\nvar count: int = 0\nfunc read() -> int:\n\treturn count\n",
    );
    let report = analyze_project(&project, &[]).unwrap();
    for rule in [
        "architecture/collection-size",
        "architecture/role-layout-conflict",
        "architecture/utility-state",
    ] {
        assert!(report.counts.contains_key(rule), "{rule}");
    }
}

#[test]
fn test_names_and_production_test_imports_are_enforced() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        "src/features/run/testing/fixture.gd",
        "class_name Fixture\nextends RefCounted\n",
    );
    write(
        root.path(),
        "src/features/run/models/run_model.gd",
        "extends RefCounted\nconst TEST = preload(\"res://src/features/run/testing/fixture.gd\")\n",
    );
    let report = analyze_project(&project, &[]).unwrap();
    assert!(report.counts.contains_key("architecture/testing-name"));
    assert!(
        report
            .counts
            .contains_key("architecture/dependency-direction")
    );
}

#[test]
fn exact_exceptions_are_checked_for_staleness() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "src/features/run/models/run_model.gd",
        "class_name Wrong\nextends RefCounted\n",
    );
    let project = project(
        root.path(),
        "[[exceptions]]\npath='src/features/run/models/run_model.gd'\nrule='architecture/class-name'\nitem='Wrong'\nreason='Fixture foreign API'\n",
    );
    assert_eq!(analyze_project(&project, &[]).unwrap().errors, 0);
    write(
        root.path(),
        "src/features/run/models/run_model.gd",
        "extends RefCounted\n",
    );
    assert!(
        analyze_project(&project, &[])
            .unwrap()
            .counts
            .contains_key("architecture/exception-stale")
    );
}

#[test]
fn dynamic_loads_require_bounded_nonstale_declarations() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "src/features/run/queries/load_run_query.gd",
        "extends RefCounted\nstatic func load_run(path: String) -> Resource:\n\treturn load(path)\n",
    );
    let project = project(
        root.path(),
        "[[dynamic_loads]]\npath='src/features/run/queries/load_run_query.gd'\nitem='load_run'\nroots=['res://src/features/run']\nreason='Authored run resources'\n",
    );
    assert_eq!(analyze_project(&project, &[]).unwrap().errors, 0);
    write(
        root.path(),
        "src/features/run/queries/load_run_query.gd",
        "extends RefCounted\nstatic func load_run(path: String) -> Resource:\n\treturn load('user://other/run_resource.tres')\n",
    );
    assert!(
        analyze_project(&project, &[])
            .unwrap()
            .counts
            .contains_key("architecture/dynamic-load-bound")
    );
    write(
        root.path(),
        "src/features/run/queries/load_run_query.gd",
        "extends RefCounted\n",
    );
    assert!(
        analyze_project(&project, &[])
            .unwrap()
            .counts
            .contains_key("architecture/dynamic-load-stale")
    );
}
#[test]
fn empty_directories_check_authored_roots_and_physical_entries() {
    let root = tempfile::tempdir().unwrap();
    let folders = [
        "addons/editor/empty",
        "docs/empty",
        "src/features/run/empty",
        "src/features/run/ignored/child",
        "src/features/run/skipped/child",
        "src/features/run/local/.godot",
        "src/features/run/nested/leaf",
        "src/features/run/marker_directory/.keep",
        "tools/empty",
    ];
    for path in folders {
        std::fs::create_dir_all(root.path().join(path)).unwrap();
    }
    for path in [
        "src/features/run/markers/.gitkeep",
        "src/features/run/markers/.keep",
        "src/features/run/metadata/.gdignore",
        "src/features/run/data/payload.bin",
    ] {
        write(root.path(), path, "");
    }
    let project = project(
        root.path(),
        "plugin_roots = ['addons/editor']\ntooling_roots = ['tools']\n\
         [[excluded]]\npath = 'src/features/run/ignored'\nreason = 'Vendor files'\n\
         [[excluded]]\npath = 'src/features/run/skipped'\nreason = 'Local output'\nskip_discovery = true\n",
    );
    let report = analyze_project(&project, &[]).unwrap();
    let paths: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|finding| finding.rule == "architecture/empty-directory")
        .map(|finding| {
            assert_eq!(finding.item, "<directory>");
            assert_eq!(finding.severity, "error");
            finding.path.as_str()
        })
        .collect();
    assert_eq!(
        paths,
        [
            "addons/editor/empty",
            "src/features/run/empty",
            "src/features/run/marker_directory/.keep",
            "src/features/run/markers",
            "src/features/run/nested/leaf",
            "tools/empty",
        ]
    );
}

#[test]
fn empty_directory_exceptions_are_exact_and_checked_for_staleness() {
    let root = tempfile::tempdir().unwrap();
    let planned = "src/features/run/planned";
    let same_owner = "src/features/run/unused";
    for path in [planned, same_owner, "src/features/other/unused"] {
        std::fs::create_dir_all(root.path().join(path)).unwrap();
    }
    let project = project(
        root.path(),
        &format!(
            "[[exceptions]]\npath = '{planned}'\nrule = 'architecture/empty-directory'\n\
             item = '<directory>'\nreason = 'Reserved by the fixture writer.'\n"
        ),
    );
    let targeted = analyze_project(&project, &[planned.into()]).unwrap();
    assert_eq!(targeted.diagnostics.len(), 1);
    assert_eq!(targeted.diagnostics[0].path, same_owner);
    assert_eq!(targeted.diagnostics[0].rule, "architecture/empty-directory");

    write(root.path(), &format!("{planned}/payload.bin"), "");
    let targeted = analyze_project(&project, &[same_owner.into()]).unwrap();
    assert!(targeted.diagnostics.iter().any(|finding| {
        finding.path == planned && finding.rule == "architecture/exception-stale"
    }));

    std::fs::remove_file(root.path().join(planned).join("payload.bin")).unwrap();
    std::fs::remove_dir(root.path().join(planned)).unwrap();
    let report = analyze_project(&project, &[]).unwrap();
    assert!(report.diagnostics.iter().any(|finding| {
        finding.path == planned && finding.rule == "architecture/exception-stale"
    }));
}
