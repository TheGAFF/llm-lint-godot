#![cfg(test)]
mod support;
use godot_llm_lint::features::{
    architecture::helpers::analyze_project,
    configuration::{
        helpers::{discover_project, native::release_for_host},
        initialize::sync_initialization as initialize,
        rules,
    },
    execution::helpers::native_checks,
};
use support::{project, write};

#[test]
fn architecture_and_native_overrides_resolve_in_declared_order() {
    let root = tempfile::tempdir().unwrap();
    let path = "src/shared/queries/value_query.gd";
    write(
        root.path(),
        path,
        "class_name Wrong\nextends RefCounted\nstatic func get_value() -> int:\n\treturn 1\n",
    );
    let configured = project(root.path(), "[rules]\n'architecture/class-name'='warn'\n");
    let report = analyze_project(&configured, &[]).unwrap();
    assert_eq!(report.errors, 0);
    assert_eq!(report.warnings, 1);
    let configured = project(
        root.path(),
        "[rules]\n'architecture/class-name'='error'\n[[overrides]]\nincludes=['src/**']\n[overrides.rules]\n'architecture/class-name'='off'\n",
    );
    assert!(
        analyze_project(&configured, &[])
            .unwrap()
            .diagnostics
            .is_empty()
    );
    assert!(rules::matches("src/**/*.gd", path));
    assert!(!rules::matches("src/*.gd", path));
    let Some(binary) = std::env::var_os("GDSTYLE_TEST_BINARY") else {
        return;
    };
    let fresh = tempfile::tempdir().unwrap();
    write(fresh.path(), "project.godot", "config_version=5\n");
    write(
        fresh.path(),
        path,
        "extends RefCounted\n\n\nstatic func get_value():\n\treturn 1\n",
    );
    initialize(
        fresh.path(),
        &[
            "--gdstyle-executable".into(),
            binary.to_string_lossy().into_owned(),
        ],
    )
    .unwrap();
    let config = fresh.path().join("llm-lint.godot.toml");
    let base = std::fs::read_to_string(&config).unwrap();
    std::fs::write(
        &config,
        format!("{base}\n[rules]\n'gdstyle/quality/type-hint'='warn'\n"),
    )
    .unwrap();
    let configured = discover_project(fresh.path(), None).unwrap();
    godot_llm_lint::features::configuration::commands::sync_project(&configured, false).unwrap();
    assert_eq!(
        native_checks(&configured, &[path.into()], false).unwrap(),
        0
    );
    std::fs::write(&config,format!("{base}\n[rules]\n'gdstyle/quality/type-hint'='off'\n[[overrides]]\nincludes=['src/**']\n[overrides.rules]\n'gdstyle/quality/type-hint'='error'\n")).unwrap();
    let configured = discover_project(fresh.path(), None).unwrap();
    godot_llm_lint::features::configuration::commands::sync_project(&configured, false).unwrap();
    assert_eq!(
        native_checks(&configured, &[path.into()], false).unwrap(),
        1
    );
}

#[test]
fn initialization_is_idempotent_and_conflicts_leave_configuration_untouched() {
    let Some(binary) = std::env::var_os("GDSTYLE_TEST_BINARY") else {
        return;
    };
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "project.godot", "config_version=5\n");
    write(
        root.path(),
        "src/shared/queries/value_query.gd",
        "extends RefCounted\n",
    );
    initialize(
        root.path(),
        &[
            "--gdstyle-executable".into(),
            binary.to_string_lossy().into_owned(),
        ],
    )
    .unwrap();
    let before = std::fs::read(root.path().join("llm-lint.godot.toml")).unwrap();
    initialize(root.path(), &[]).unwrap();
    assert_eq!(
        before,
        std::fs::read(root.path().join("llm-lint.godot.toml")).unwrap()
    );
    write(root.path(), ".gdstyle.toml", "legacy=true\n");
    assert!(initialize(root.path(), &[]).is_err());
    assert_eq!(
        before,
        std::fs::read(root.path().join("llm-lint.godot.toml")).unwrap()
    );
    let fresh = tempfile::tempdir().unwrap();
    write(fresh.path(), "project.godot", "config_version=5\n");
    write(
        fresh.path(),
        "src/shared/queries/value_query.gd",
        "extends RefCounted\n",
    );
    write(fresh.path(), "bad.zip", "broken archive");
    assert!(
        initialize(
            fresh.path(),
            &["--gdstyle-archive".into(), "bad.zip".into()]
        )
        .is_err()
    );
    assert!(!fresh.path().join("llm-lint.godot.toml").exists());
    assert!(!fresh.path().join("gdstyle.toml").exists());
}

#[test]
fn initialization_upgrades_both_legacy_pins_after_verifying_the_release() {
    let binary = std::env::var_os("GDSTYLE_TEST_BINARY")
        .expect("Provide the pinned executable for migration verification");
    let legacy_digest = "0".repeat(64);
    let policy = "version=1\nsource_roots=['src']\n\n[style]\nmax_line_length=90\n\n[rules]\n'gdstyle/quality/type-hint'='warn'\n\n[[overrides]]\nincludes=['src/shared/**']\n[overrides.rules]\n'gdstyle/quality/type-hint'='off'";
    for pin in [
        "auto=true\nversion='0.3.1'".to_owned(),
        format!("executable='legacy.exe'\nversion='0.3.1'\nsha256='{legacy_digest}'"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let path = "src/shared/nodes/report_node.gd";
        write(root, "project.godot", "config_version=5\n");
        let script = "extends Node\n\n\nfunc report(value: int) -> void:\n\tprint( value )\n";
        write(root, path, script);
        write(root, "legacy.exe", "untrusted legacy checkout file");
        let previous = format!("{policy}\n\n[gdstyle]\n{pin}\n");
        let previous_style =
            "# Generated by godot-llm-lint; edit llm-lint.godot.toml.\nuse_tabs = true\n";
        write(root, "llm-lint.godot.toml", &previous);
        write(root, "gdstyle.toml", previous_style);
        let config = root.join("llm-lint.godot.toml");
        let style = root.join("gdstyle.toml");

        let error = discover_project(root, None).err().unwrap();
        assert!(error.to_string().contains("Run godot-llm-lint init"));
        write(root, "bad.zip", "invalid archive");
        assert!(initialize(root, &["--gdstyle-archive".into(), "bad.zip".into()]).is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), previous);
        assert_eq!(std::fs::read_to_string(&style).unwrap(), previous_style);

        initialize(
            root,
            &[
                "--gdstyle-executable".into(),
                binary.to_string_lossy().into_owned(),
            ],
        )
        .unwrap();
        let upgraded = std::fs::read_to_string(config).unwrap();
        assert!(upgraded.starts_with(policy));
        let project = discover_project(root, None).unwrap();
        let native = project.settings.gdstyle.as_ref().unwrap();
        assert!(native.auto);
        assert_eq!(native.version, "0.8.0");
        assert!(native.executable.is_empty() && native.sha256.is_empty());
        assert!(
            std::fs::read_to_string(style)
                .unwrap()
                .contains("\"format/extra-spacing\" = \"error\"")
        );
        godot_llm_lint::features::configuration::commands::sync_project(&project, true).unwrap();
        assert_eq!(native_checks(&project, &[path.into()], true).unwrap(), 1);
        assert_eq!(std::fs::read_to_string(root.join(path)).unwrap(), script);
    }
}

#[test]
fn automatic_configuration_restores_the_platform_cache_without_changing_committed_settings() {
    let binary = std::env::var_os("GDSTYLE_TEST_BINARY")
        .expect("Provide the pinned executable for setup verification");
    let root = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let release = release_for_host().unwrap();
    let init = |from_file: bool| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"));
        command
            .current_dir(root.path())
            .env("GODOT_LLM_LINT_CACHE_DIR", cache.path())
            .arg("init");
        if from_file {
            command.arg("--gdstyle-executable").arg(&binary);
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    write(root.path(), "project.godot", "config_version=5\n");
    write(
        root.path(),
        "src/shared/queries/value_query.gd",
        "extends RefCounted\n",
    );
    let config = "version=1\nsource_roots=['src']\n[gdstyle]\nauto=true\nversion='0.8.0'\n";
    write(root.path(), "llm-lint.godot.toml", config);
    init(true);
    let before = std::fs::read(root.path().join("llm-lint.godot.toml")).unwrap();
    let path = cache
        .path()
        .join("gdstyle/0.8.0")
        .join(release.target)
        .join(release.executable);
    assert!(path.is_file());
    std::fs::write(&path, "corrupted cache").unwrap();
    init(true);
    assert_eq!(
        before,
        std::fs::read(root.path().join("llm-lint.godot.toml")).unwrap()
    );
    init(false);
    assert_eq!(
        before,
        std::fs::read(root.path().join("llm-lint.godot.toml")).unwrap()
    );
}

#[test]
fn unknown_producer_rules_cannot_disappear_from_reports() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "src/shared/queries/value_query.gd",
        "extends RefCounted\nstatic func get_value() -> int:\n\treturn 1\n",
    );
    let configured = project(root.path(), "");
    let mut corpus = godot_llm_lint::features::architecture::helpers::corpus(&configured).unwrap();
    corpus.sources[0]
        .diagnostics
        .push(godot_llm_lint::shared::helpers::issue(
            "src/shared/queries/value_query.gd",
            1,
            1,
            "architecture/unregistered-rule",
            "get_value",
            "Unknown producer rule",
            "Register the rule",
        ));
    let error =
        godot_llm_lint::features::architecture::helpers::analyze_corpus(&configured, &corpus, &[])
            .unwrap_err();
    assert!(error.to_string().contains("Unregistered diagnostic rule"));
}
