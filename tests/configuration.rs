#![cfg(test)]
mod support;
use godot_llm_lint::features::configuration::{
    commands::sync_project,
    helpers::{discover_project, target_paths},
};
use support::{project, write};

#[test]
fn sync_is_deterministic_and_nested_discovery_preserves_root_paths() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "[style]\nmax_line_length=100\n");
    let filename = root.path().join("gdstyle.toml");
    let first = std::fs::read(&filename).unwrap();
    sync_project(&project, false).unwrap();
    assert_eq!(first, std::fs::read(&filename).unwrap());
    let windows = String::from_utf8(first.clone())
        .unwrap()
        .replace('\n', "\r\n");
    std::fs::write(&filename, &windows).unwrap();
    sync_project(&project, true).unwrap();
    sync_project(&project, false).unwrap();
    assert_eq!(windows.as_bytes(), std::fs::read(&filename).unwrap());
    let nested = root.path().join("src/features/run");
    std::fs::create_dir_all(&nested).unwrap();
    assert_eq!(discover_project(&nested, None).unwrap().root, project.root);
    assert!(target_paths(&project, &["../other".into()]).is_err());
    assert!(target_paths(&project, &["missing".into()]).is_err());
    write(root.path(), "gdstyle.toml", "stale=true\n");
    assert!(sync_project(&project, true).is_err());
    sync_project(&project, false).unwrap();
    write(root.path(), ".gdstyle.toml", "");
    assert!(sync_project(&project, false).is_err());
    assert!(sync_project(&project, true).is_err());
}

#[test]
fn invalid_configuration_never_silently_weakens_policy() {
    let invalid = [
        "version=2",
        "collection_max_code_lines=0",
        "unknown=true",
        "source_roots=[]",
        "source_roots=['missing']",
        "source_roots=['src','src/features']",
        "source_roots=['../src']",
        "source_roots=['src\\\\other']",
        "[[entrypoints]]\npath='src'\nreason=''",
        "[[excluded]]\npath='absent'\nreason='upstream'",
        "[[excluded]]\npath='../outside'\nreason='generated'\noptional=true",
        "[[excluded]]\npath='absent'\nreason=''\noptional=true",
        "[[entrypoints]]\npath='absent.gd'\nreason='entry'\noptional=true",
        "[duplicates]\nwarning_tokens=0",
        "[duplicates]\nwarning_lines=0",
        "[duplicates]\nerror_tokens=10",
        "[duplicates]\nerror_lines=2",
        "[duplicates]\nsimilarity=1.0",
        "[duplicates]\nsimilarity=0.0",
        "[style]\nunknown=3",
        "[style]\nmax_parameters=0",
        "[gdstyle]\nexecutable='gdstyle'\nversion='latest'\nsha256='abc'",
        "[gdstyle]\nexecutable=''\nversion='0.3.1'\nsha256='0000000000000000000000000000000000000000000000000000000000000000'",
        "[[exceptions]]\npath='src'\nrule='wrong'\nitem='one'\nreason='test'",
        "[[dynamic_loads]]\npath='src/load.gd'\nitem='load_all'\nroots=[]\nreason='test'",
        "[[dynamic_loads]]\npath='src/load.gd'\nitem='load_all'\nroots=['other']\nreason='test'",
        "[[dynamic_loads]]\npath='src/load.gd'\nitem='load_all'\nroots=['res://absent']\nreason='test'",
    ];
    for settings in invalid {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "project.godot", "config_version=5\n");
        write(root.path(), "src/load.gd", "extends RefCounted\n");
        std::fs::create_dir_all(root.path().join("src/features")).unwrap();
        write(root.path(), "llm-lint.godot.toml", settings);
        assert!(discover_project(root.path(), None).is_err(), "{settings}");
    }
    let empty = tempfile::tempdir().unwrap();
    assert!(discover_project(empty.path(), None).is_err());
    write(empty.path(), "llm-lint.godot.toml", "version=1\n");
    assert!(discover_project(empty.path(), None).is_err());
}

#[test]
fn optional_generated_exclusions_work_before_and_after_generation() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "src/shared/data/payload.bin", "");
    let project = project(
        root.path(),
        "[[excluded]]\npath='docs/review'\nreason='Generated review assets'\noptional=true\n",
    );
    assert_eq!(
        godot_llm_lint::features::architecture::helpers::analyze_project(&project, &[])
            .unwrap()
            .errors,
        0
    );
    write(root.path(), "docs/review/screenshot.png", "");
    write(root.path(), "docs/review/README.md", "");
    assert_eq!(
        godot_llm_lint::features::architecture::helpers::analyze_project(&project, &[])
            .unwrap()
            .errors,
        0
    );
    sync_project(&project, true).unwrap();
}

#[test]
fn default_discovery_preserves_the_serialized_history_policy() {
    let root = tempfile::tempdir().unwrap();
    let config = "[[excluded]]\npath='scratch'\nreason='Generated'\noptional=true\n";
    let original = serde_json::json!({"path": "scratch", "reason": "Generated", "optional": true});
    for flag in ["", "skip_discovery=false\n", "skip_discovery=true\n"] {
        let project = project(root.path(), &format!("{config}{flag}"));
        let serialized = serde_json::to_value(&project.settings.excluded[0]).unwrap();
        if flag == "skip_discovery=true\n" {
            assert_eq!(serialized["skip_discovery"], true);
        } else {
            assert_eq!(serialized, original);
        }
    }
}

#[test]
fn duplicate_declarations_and_absolute_targets_fail() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    let other = tempfile::tempdir().unwrap();
    assert!(target_paths(&project, &[other.path().to_string_lossy().into_owned()]).is_err());
    for declaration in [
        "[[dynamic_loads]]\npath='src/run.gd'\nitem='load_run'\nroots=['user://test']\nreason='test'\n",
        "[[exceptions]]\npath='src/run.gd'\nitem='load_run'\nrule='architecture/dynamic-load'\nreason='test'\n",
    ] {
        write(root.path(), "src/run.gd", "extends RefCounted\n");
        write(
            root.path(),
            "llm-lint.godot.toml",
            &(declaration.to_owned() + declaration),
        );
        assert!(discover_project(root.path(), None).is_err());
    }
}
