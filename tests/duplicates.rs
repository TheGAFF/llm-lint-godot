#![cfg(test)]
#[path = "support/rule_examples.rs"]
mod rule_examples;
mod support;
use rule_examples::analyze_duplicates;
use support::{project, write};

fn calculation(name: &str, variable: &str) -> String {
    let mut text = format!(
        "extends RefCounted\nstatic func {name}({variable}: int) -> int:\n\tvar total: int = {variable}\n"
    );
    for index in 0..18 {
        text.push_str(&format!("\ttotal = (total + {index}) * 2 - {variable}\n"));
    }
    text.push_str("\treturn total\n");
    text
}

#[test]
fn warnings_are_grouped_once_and_details_are_opt_in() {
    let root = tempfile::tempdir().unwrap();
    let settings =
        "[duplicates]\nwarning_tokens=10\nwarning_lines=3\nerror_tokens=900\nerror_lines=99\n";
    let project = project(root.path(), settings);
    let paths = [
        "src/features/run/queries/value_query.gd",
        "src/features/run/testing/test_value.gd",
        "src/features/run/tests/value_test_factory.gd",
        "src/features/run/testing/value_capture_test_harness.gd",
        "src/features/run/captures/frame_node.gd",
    ];
    for path in paths {
        write(root.path(), path, &calculation("compute_value", "input"));
    }
    let full = analyze_duplicates(&project, &[]).unwrap();
    let counts = &full.summary.warnings_by_category;
    assert_eq!((counts.production, counts.test, counts.capture), (4, 5, 1));
    assert_eq!(full.summary.warnings, 10);
    let scoped = analyze_duplicates(&project, &[paths[3].into()]).unwrap();
    let counts = &scoped.summary.warnings_by_category;
    assert_eq!((counts.production, counts.test, counts.capture), (1, 2, 1));
    assert_eq!(scoped.summary.warnings, 4);
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"))
            .current_dir(root.path())
            .arg("duplicates")
            .args(args)
            .output()
            .unwrap()
    };
    let default = run(&[]);
    assert!(default.status.success());
    assert!(default.stderr.is_empty());
    assert!(String::from_utf8_lossy(&default.stdout).contains("4 production, 5 test, 1 capture"));
    let detailed = run(&["--duplicate-warnings", "capture"]);
    assert!(detailed.status.success());
    let stderr = String::from_utf8(detailed.stderr).unwrap();
    assert!(stderr.contains("Duplicate warnings [capture]"));
    assert!(!stderr.contains("Duplicate warnings [test]"));
    assert!(stderr.contains(paths[3]) && stderr.contains(paths[4]));
    let invalid = run(&["--duplicate-warnings", "capture", "--json"]);
    assert_eq!(invalid.status.code(), Some(2));
    let json = run(&["--json"]);
    assert!(json.status.success());
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(
        json["summary"]["warnings_by_category"],
        serde_json::json!({
            "production": 4, "test": 5, "capture": 1,
        })
    );
    assert!(
        json["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| {
                finding["category"] == "production"
                    && finding["occurrences"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|occurrence| occurrence["category"] == "capture")
            })
    );
    let override_config = format!(
        "{settings}\n[[overrides]]\nincludes=['{}']\n[overrides.rules]\n'duplicates/exact'='error'\n",
        paths[1],
    );
    let overridden = support::project(root.path(), &override_config);
    let report = analyze_duplicates(&overridden, &[]).unwrap();
    assert_eq!((report.summary.errors, report.summary.warnings), (4, 6));
    let counts = &report.summary.warnings_by_category;
    assert_eq!((counts.production, counts.test, counts.capture), (3, 2, 1));
}

#[test]
fn exact_errors_renamed_warnings_and_targets_keep_full_corpus() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        "src/features/one/calculators/value_calculator.gd",
        &calculation("compute_value", "input"),
    );
    write(
        root.path(),
        "src/features/two/calculators/value_calculator.gd",
        &calculation("compute_other", "input"),
    );
    let report = analyze_duplicates(&project, &["src/features/one".into()]).unwrap();
    assert!(report.summary.errors > 0);
    assert_eq!(report.diagnostics[0].kind, "exact");
    assert!(
        report.diagnostics[0]
            .occurrences
            .iter()
            .any(|occurrence| occurrence.path.contains("two"))
    );
    assert!(
        report.diagnostics[0]
            .occurrences
            .iter()
            .all(|occurrence| occurrence.tokens >= 100 && occurrence.code_lines >= 10)
    );
    write(
        root.path(),
        "src/features/two/calculators/value_calculator.gd",
        &calculation("compute_other", "value"),
    );
    let report = analyze_duplicates(&project, &[]).unwrap();
    assert_eq!(report.summary.errors, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.kind == "renamed")
    );
    assert_eq!(
        serde_json::to_string(&report).unwrap(),
        serde_json::to_string(&analyze_duplicates(&project, &[]).unwrap()).unwrap()
    );
}

#[test]
fn structural_candidates_and_state_blockers_remain_advisory() {
    let root = tempfile::tempdir().unwrap();
    let project = project(
        root.path(),
        "[duplicates]\nwarning_tokens=10\nwarning_lines=3\nsimilarity=0.75\n",
    );
    let left = "extends Node\nfunc calculate_one(input: int) -> int:\n\tvar total: int = input + 8\n\tif total > 10:\n\t\ttotal += 7\n\treturn total * 3\n";
    let right = left
        .replace("calculate_one", "calculate_two")
        .replace("+ 8", "+ 9")
        .replace("> 10", "> 11")
        .replace("+= 7", "+= 6")
        .replace("* 3", "* 4");
    write(root.path(), "src/features/ui/nodes/one_node.gd", left);
    write(root.path(), "src/features/ui/nodes/two_node.gd", &right);
    let report = analyze_duplicates(&project, &[]).unwrap();
    assert_eq!(report.summary.errors, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.kind == "similar")
    );
    assert!(
        report
            .diagnostics
            .iter()
            .all(|finding| finding.guidance.kind == "review"
                && !finding.guidance.blockers.is_empty())
    );
}

#[test]
fn static_resource_data_comments_and_short_accessors_do_not_form_units() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    for name in ["one", "two"] {
        write(
            root.path(),
            &format!("src/features/content/resources/{name}.tres"),
            "[gd_resource format=3]\n[resource]\nvalues=[1,2,3,4,5]\n",
        );
        write(
            root.path(),
            &format!("src/shared/queries/{name}_query.gd"),
            "extends RefCounted\n# Static data is not executable.\nconst DATA = [1,2,3,4,5]\nstatic func get_value() -> int:\n\treturn 1\n",
        );
    }
    let report = analyze_duplicates(&project, &[]).unwrap();
    assert_eq!(report.summary.analyzed_units, 2);
    assert!(report.diagnostics.is_empty());
    write(
        root.path(),
        "src/shared/queries/bad_query.gd",
        "func bad(:\n",
    );
    assert!(analyze_duplicates(&project, &[]).is_err());
}

#[test]
fn exclusion_disabling_and_warning_line_thresholds_are_respected() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "vendor/one.gd",
        &calculation("compute_value", "input"),
    );
    let project = project(
        root.path(),
        "[[excluded]]\npath='vendor'\nreason='Upstream implementation'\n",
    );
    write(
        root.path(),
        "src/shared/calculators/value_calculator.gd",
        &calculation("compute_value", "input"),
    );
    assert!(
        analyze_duplicates(&project, &[])
            .unwrap()
            .diagnostics
            .is_empty()
    );
    let disabled = support::project(root.path(), "[duplicates]\nenabled=false\n");
    assert!(analyze_duplicates(&disabled, &[]).is_err());
}

#[test]
fn private_owner_and_capture_guidance_does_not_assume_portability() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    let first = calculation("compute_one", "input");
    let second = calculation("compute_two", "input");
    let joined = first.clone() + second.trim_start_matches("extends RefCounted\n");
    write(
        root.path(),
        "src/features/run/calculators/one_calculator.gd",
        &joined,
    );
    let report = analyze_duplicates(&project, &[]).unwrap();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.guidance.kind == "private")
    );
    write(
        root.path(),
        "src/features/run/calculators/one_calculator.gd",
        &first,
    );
    write(
        root.path(),
        "src/features/run/calculators/two_calculator.gd",
        &second,
    );
    let report = analyze_duplicates(&project, &[]).unwrap();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|finding| finding.guidance.kind == "owner")
    );
    for name in ["one", "two"] {
        write(
            root.path(),
            &format!("src/features/run/calculators/{name}_calculator.gd"),
            &calculation("compute_value", "input")
                .replace(
                    "extends RefCounted",
                    "extends RefCounted\nconst BASE: int = 7",
                )
                .replace("* 2", "* BASE"),
        );
    }
    let report = analyze_duplicates(&project, &[]).unwrap();
    assert!(report.diagnostics.iter().any(|finding| {
        finding
            .guidance
            .blockers
            .iter()
            .any(|blocker| blocker.contains("BASE"))
    }));
}
