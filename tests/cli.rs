#![cfg(test)]
mod support;
use godot_llm_lint::{
    app::handlers::handle_cli,
    features::{configuration::helpers::discover_project, execution::helpers::native_checks},
};
use sha2::{Digest, Sha256};
use support::{project, write};

fn cli(root: &std::path::Path, args: &[&str]) -> i32 {
    let mut arguments = args
        .iter()
        .map(|argument| (*argument).to_owned())
        .collect::<Vec<_>>();
    arguments.extend([
        "--config".into(),
        root.join("llm-lint.godot.toml")
            .to_string_lossy()
            .into_owned(),
    ]);
    handle_cli(arguments)
}

#[test]
fn executable_reports_machine_readable_results_and_a_nested_working_directory() {
    let root = tempfile::tempdir().unwrap();
    project(root.path(), "");
    write(
        root.path(),
        "src/shared/queries/value_query.gd",
        "extends RefCounted\nstatic func get_value() -> int:\n\treturn 1\n",
    );
    let binary = env!("CARGO_BIN_EXE_godot-llm-lint");
    let result = std::process::Command::new(binary)
        .current_dir(root.path().join("src"))
        .args(["architecture", ".", "--json"])
        .output()
        .unwrap();
    assert!(result.status.success());
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["errors"], 0);
    let result = std::process::Command::new(binary)
        .current_dir(root.path())
        .args(["architecture", "missing", "--json"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap()["fatal"].is_string()
    );
}

#[test]
fn help_exit_codes_write_flags_and_fatal_json_are_stable() {
    assert_eq!(handle_cli(vec![]), 0);
    assert_eq!(handle_cli(vec!["--help".into()]), 0);
    assert_eq!(handle_cli(vec!["--version".into()]), 0);
    assert_eq!(handle_cli(vec!["unknown".into()]), 2);
    assert_eq!(handle_cli(vec!["check".into(), "--config".into()]), 2);
    let root = tempfile::tempdir().unwrap();
    project(root.path(), "");
    write(
        root.path(),
        "src/shared/queries/value_query.gd",
        "extends RefCounted\nstatic func get_value() -> int:\n\treturn 1\n",
    );
    for args in [
        &["sync"][..],
        &["sync", "--check"],
        &["architecture"],
        &["architecture", "--json"],
        &["architecture", ".", "--json"],
        &["duplicates"],
        &["duplicates", "--json"],
    ] {
        assert_eq!(cli(root.path(), args), 0);
    }
    for args in [
        &["sync", "--write"][..],
        &["check", "--fix"],
        &["architecture", "--json", "--json"],
        &["architecture", "--timings", "--timings"],
        &["sync", "--check", "--timings"],
        &["lint", "--json"],
        &["check"],
        &["architecture", "missing", "--json"],
    ] {
        assert_eq!(cli(root.path(), args), 2);
    }
    write(
        root.path(),
        "src/shared/queries/value_query.gd",
        "extends RefCounted\nstatic func validate_value() -> bool:\n\treturn true\n",
    );
    assert_eq!(cli(root.path(), &["architecture"]), 1);
    assert_eq!(cli(root.path(), &["architecture", "."]), 1);
    write(root.path(), "gdstyle.toml", "stale");
    assert_eq!(cli(root.path(), &["architecture", "--json"]), 2);
}

#[test]
fn phase_timings_are_opt_in_and_leave_json_and_status_unchanged() {
    let root = tempfile::tempdir().unwrap();
    project(root.path(), "");
    write(
        root.path(),
        "src/shared/queries/value_query.gd",
        "extends RefCounted\nstatic func get_value() -> int:\n\treturn 1\n",
    );
    let invoke = |args: &[&str], timings: bool| {
        std::process::Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"))
            .current_dir(root.path())
            .args(args)
            .args(timings.then_some("--timings"))
            .output()
            .unwrap()
    };
    for (args, status, phase) in [
        (&["architecture", "--json"][..], 0, "architecture"),
        (&["duplicates", "--json"][..], 0, "duplicates"),
        (
            &["architecture", "missing", "--json"][..],
            2,
            "architecture",
        ),
    ] {
        let plain = invoke(args, false);
        let timed = invoke(args, true);
        assert_eq!(plain.status.code(), Some(status));
        assert_eq!(timed.status, plain.status);
        assert_eq!(timed.stdout, plain.stdout);
        assert!(plain.stderr.is_empty());
        let stderr = String::from_utf8(timed.stderr).unwrap();
        for stage in ["discovery", "configuration", "corpus", phase, "total"] {
            assert!(stderr.contains(&format!("{stage}=")), "{stderr}");
        }
    }
}

#[test]
fn native_tool_integrity_lint_format_and_no_mutation() {
    let Some(binary) = std::env::var_os("GDSTYLE_TEST_BINARY") else {
        return;
    };
    let bytes = std::fs::read(&binary).unwrap();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let root = tempfile::tempdir().unwrap();
    let config = format!("[gdstyle]\nexecutable='gdstyle.exe'\nversion='0.8.0'\nsha256='{hash}'\n");
    let project = project(root.path(), &config);
    let path = "src/shared/queries/value_query.gd";
    write(
        root.path(),
        path,
        "extends RefCounted\n\n\nstatic func get_value() -> int:\n\treturn 1\n",
    );
    assert_eq!(native_checks(&project, &[], true).unwrap(), 0);
    assert_eq!(native_checks(&project, &[path.into()], true).unwrap(), 0);
    assert_eq!(cli(root.path(), &["check"]), 0);
    assert_eq!(cli(root.path(), &["lint", path]), 0);
    for command in ["check", "lint"] {
        let timed = std::process::Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"))
            .current_dir(root.path())
            .args([command, path, "--timings"])
            .output()
            .unwrap();
        assert!(timed.status.success());
        let stderr = String::from_utf8(timed.stderr).unwrap();
        for stage in ["architecture", "native", "duplicates", "total"] {
            assert!(stderr.contains(&format!("{stage}=")), "{stderr}");
        }
    }
    let empty = "src/shared/fixtures";
    std::fs::create_dir(root.path().join(empty)).unwrap();
    for command in ["architecture", "check"] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"))
            .current_dir(root.path())
            .args([command, empty, "--json"])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        let architecture = if command == "check" {
            &report["architecture"]
        } else {
            &report
        };
        assert_eq!(
            architecture["diagnostics"][0]["rule"],
            "architecture/empty-directory"
        );
        assert_eq!(architecture["diagnostics"][0]["path"], empty);
        assert!(root.path().join(empty).is_dir());
    }
    let bad = "extends RefCounted\nstatic func get_value():\n    return 1\n";
    write(root.path(), path, bad);
    assert_eq!(native_checks(&project, &[path.into()], true).unwrap(), 1);
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"))
        .current_dir(root.path())
        .args(["check", path, "--json"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(
        report["native"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| { finding["path"] == path && finding["rule"] == "formatting/check" })
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join(path)).unwrap(),
        bad
    );
    write(root.path(), "gdstyle.exe", "untrusted checkout file");
    assert_eq!(native_checks(&project, &[path.into()], false).unwrap(), 1);
    write(
        root.path(),
        "llm-lint.godot.toml",
        &config.replace(&hash, &"0".repeat(64)),
    );
    let wrong = discover_project(root.path(), None).unwrap();
    assert!(native_checks(&wrong, &[path.into()], true).is_err());
}

#[test]
fn native_long_unicode_paths_check_every_batch_without_writing() {
    let Some(binary) = std::env::var_os("GDSTYLE_TEST_BINARY") else {
        return;
    };
    let bytes = std::fs::read(&binary).unwrap();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let root = tempfile::tempdir().unwrap();
    let project = project(
        root.path(),
        &format!("[gdstyle]\nexecutable='gdstyle.exe'\nversion='0.8.0'\nsha256='{hash}'\n"),
    );
    let good = "extends RefCounted\n\n\nstatic func get_value() -> int:\n\treturn 1\n";
    let paths: Vec<String> = (0..500)
        .map(|index| {
            format!(
                "src/shared/queries/日本語 space/{}_{index}.gd",
                "long".repeat(20)
            )
        })
        .collect();
    for path in &paths {
        write(root.path(), path, good);
    }
    assert_eq!(native_checks(&project, &paths, false).unwrap(), 0);
    let bad = "extends RefCounted\nstatic func get_value():\n    return 1\n";
    write(root.path(), &paths[400], bad);
    assert_eq!(native_checks(&project, &paths, false).unwrap(), 1);
    for (index, path) in paths.iter().enumerate() {
        assert_eq!(
            std::fs::read_to_string(root.path().join(path)).unwrap(),
            if index == 400 { bad } else { good }
        );
    }
}
