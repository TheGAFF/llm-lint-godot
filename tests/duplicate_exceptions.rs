#![cfg(test)]
mod support;
use godot_llm_lint::features::{
    configuration::models::{DuplicateException, DuplicateMember},
    duplicates::helpers::analyze_duplicates,
    parsing::parsers::parse_script,
};
use support::{project, write};

const CODE: &str = "extends RefCounted\nstatic func calculate(input: int) -> int:\n\tvar total: int = input + 1\n\ttotal += 2\n\ttotal *= 3\n\treturn total\n";

fn entry(paths: &[&str]) -> DuplicateException {
    let fingerprint = parse_script("one.gd", CODE.into()).unwrap().functions[0]
        .fingerprint
        .clone();
    DuplicateException {
        id: "reviewed_data".into(),
        rules: vec!["duplicates/exact".into()],
        reason: "Reviewed independent contract fixtures retain their own expected values".into(),
        members: paths
            .iter()
            .map(|path| DuplicateMember {
                path: (*path).into(),
                item: "calculate".into(),
                fingerprint: fingerprint.clone(),
            })
            .collect(),
    }
}

#[test]
fn both_endpoints_must_be_reviewed_and_new_partners_remain_visible() {
    let root = tempfile::tempdir().unwrap();
    let mut project = project(
        root.path(),
        "[duplicates]\nwarning_tokens=10\nwarning_lines=3\n",
    );
    let paths = [
        "src/features/ui/testing/one_test_harness.gd",
        "src/features/ui/testing/two_test_harness.gd",
        "src/features/ui/testing/three_test_harness.gd",
    ];
    for path in paths {
        write(root.path(), path, CODE);
    }
    project
        .settings
        .duplicate_exceptions
        .push(entry(&paths[..2]));
    let report = analyze_duplicates(&project, &[]).unwrap();
    assert_eq!(report.summary.suppressed, 1);
    assert_eq!(report.summary.warnings, 2);
    assert_eq!(report.summary.warnings_by_category.test, 2);
    assert!(
        report
            .diagnostics
            .iter()
            .all(|f| f.occurrences.iter().any(|o| o.path == paths[2]))
    );
    assert_eq!(report.suppressed[0].exception_id, "reviewed_data");
    assert!(!report.suppressed[0].reason.is_empty());
    assert_eq!(
        serde_json::to_string(&report).unwrap(),
        serde_json::to_string(&analyze_duplicates(&project, &[]).unwrap()).unwrap()
    );
    let targeted = analyze_duplicates(&project, &[paths[0].into()]).unwrap();
    assert_eq!(targeted.summary.suppressed, 1);
    assert_eq!(targeted.summary.warnings, 1);

    let outside = analyze_duplicates(&project, &[paths[2].into()]).unwrap();
    assert_eq!(outside.summary.suppressed, 0);
    assert_eq!(
        serde_json::to_value(outside.diagnostics).unwrap(),
        serde_json::to_value(report.diagnostics).unwrap()
    );
}

#[test]
fn changed_missing_ambiguous_and_unused_approvals_are_errors_even_outside_targets() {
    let root = tempfile::tempdir().unwrap();
    let mut project = project(
        root.path(),
        "[duplicates]\nwarning_tokens=10\nwarning_lines=3\n",
    );
    let paths = [
        "src/features/ui/testing/one_test_harness.gd",
        "src/features/ui/testing/two_test_harness.gd",
    ];
    for path in paths {
        write(root.path(), path, CODE);
    }
    let unrelated = "src/features/run/testing/unrelated_test_harness.gd";
    write(root.path(), unrelated, "extends RefCounted\n");
    project.settings.duplicate_exceptions.push(entry(&paths));
    write(root.path(), paths[1], &CODE.replace("+ 1", "+ 9"));
    assert!(
        analyze_duplicates(&project, &[unrelated.into()])
            .unwrap_err()
            .to_string()
            .contains("Stale duplicate exception")
    );
    write(root.path(), paths[1], "extends RefCounted\n");
    assert!(analyze_duplicates(&project, &[]).is_err());
    write(
        root.path(),
        paths[1],
        &(CODE.to_owned() + CODE.trim_start_matches("extends RefCounted\n")),
    );
    assert!(analyze_duplicates(&project, &[]).is_err());
    write(root.path(), paths[1], CODE);
    project.settings.duplicate_exceptions[0].rules = vec!["duplicates/similar".into()];
    let error = analyze_duplicates(&project, &[]).unwrap_err().to_string();
    assert!(error.contains("remove unused members or rules"));
    assert_eq!(
        analyze_duplicates(&project, &[unrelated.into()])
            .unwrap_err()
            .to_string(),
        error
    );
}

#[test]
fn exceptions_cannot_suppress_errors_and_overlapping_approvals_fail() {
    let root = tempfile::tempdir().unwrap();
    let mut project = project(
        root.path(),
        "[duplicates]\nwarning_tokens=10\nwarning_lines=3\n",
    );
    let paths = [
        "src/features/ui/testing/one_test_harness.gd",
        "src/features/ui/testing/two_test_harness.gd",
    ];
    for path in paths {
        write(root.path(), path, CODE);
    }
    let unrelated = "src/features/run/testing/unrelated_test_harness.gd";
    write(root.path(), unrelated, "extends RefCounted\n");
    project.settings.duplicate_exceptions.push(entry(&paths));
    project.settings.duplicate_exceptions.push(entry(&paths));
    for targets in [vec![], vec![unrelated.into()]] {
        assert_eq!(
            analyze_duplicates(&project, &targets)
                .unwrap_err()
                .to_string(),
            "Overlapping duplicate exceptions"
        );
    }
    project.settings.duplicate_exceptions.pop();
    project.settings.duplicates.error_tokens = 10;
    project.settings.duplicates.error_lines = 3;
    assert!(analyze_duplicates(&project, &[]).is_err());
    project.settings.duplicate_exceptions.clear();
    let escalated = analyze_duplicates(&project, &[]).unwrap();
    assert_eq!(escalated.summary.errors, 1);
    assert_eq!(escalated.summary.warnings_by_category.test, 0);
}

#[test]
fn configuration_rejects_broad_or_unreasoned_exceptions() {
    let root = tempfile::tempdir().unwrap();
    for suffix in [
        "[[duplicate_exceptions]]\nid='a'\nrules=['duplicates/*']\nreason='review'\nmembers=[]\n",
        "[[duplicate_exceptions]]\nid='a'\nrules=['duplicates/exact']\nreason=''\nmembers=[]\n",
    ] {
        let _ = project(root.path(), "");
        std::fs::write(
            root.path().join("llm-lint.godot.toml"),
            format!("version=1\nsource_roots=['src']\n{suffix}"),
        )
        .unwrap();
        assert!(
            godot_llm_lint::features::configuration::helpers::discover_project(root.path(), None)
                .is_err()
        );
    }
}
