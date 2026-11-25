#![cfg(test)]
mod support;
use godot_llm_lint::features::{
    architecture::helpers::{analyze_project, corpus},
    configuration::helpers::discover_project,
    duplicates::helpers::analyze_duplicates,
};
use support::{project, write};

#[test]
fn every_role_and_owner_accepts_nested_documents_without_runtime_parsing() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("addons/editor")).unwrap();
    std::fs::create_dir_all(root.path().join("tools")).unwrap();
    let project = project(
        root.path(),
        "plugin_roots=['addons/editor']\ntooling_roots=['tools']\n",
    );
    let mut paths = Vec::new();
    for owner in [
        "src/features/run",
        "src/app",
        "src/shared",
        "addons/editor",
        "tools",
    ] {
        for (folder, role) in [
            ("specifications", "specification"),
            ("guides", "guide"),
            ("references", "reference"),
            ("standards", "standard"),
            ("decisions", "decision"),
            ("verification", "verification"),
        ] {
            let path = format!("{owner}/docs/{folder}/saves/checkpoint_{role}.md");
            write(root.path(), &path, &valid_document(role));
            paths.push(path);
        }
    }
    review_documents(root.path(), &paths);
    let inventory = corpus(&project).unwrap();
    assert_eq!(inventory.documents.len(), 30);
    assert_eq!(inventory.sources.len(), 1);
    assert!(
        inventory.documents.iter().all(|doc| !doc.text.is_empty()
            && doc.references.is_empty()
            && doc.functions.is_empty())
    );
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(report.errors, 0, "{:?}", report.diagnostics);
    assert_eq!(report.scanned_files, 31);
    assert_eq!(
        analyze_duplicates(&project, &[])
            .unwrap()
            .summary
            .analyzed_units,
        0
    );
}

#[test]
fn suffix_precedes_folder_and_recommendations_preserve_topics() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    let wrong = "src/features/run/docs/guides/saves/resume_specification.md";
    write(root.path(), wrong, "");
    write(
        root.path(),
        "src/features/run/docs/standards/saves/Save-Rules.md",
        "",
    );
    let report = analyze_project(&project, &[]).unwrap();
    let placement = report
        .diagnostics
        .iter()
        .find(|item| item.path == wrong && item.rule == "architecture/role-placement")
        .unwrap();
    assert_eq!(placement.rule, "architecture/role-placement");
    assert_eq!(
        placement.expected,
        "src/features/run/docs/specifications/saves/resume_specification.md"
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|item| item.rule == "architecture/role-filename"
                && item.expected == "src/features/run/docs/standards/saves/save_rules_standard.md")
    );
}

#[test]
fn unowned_unknown_uppercase_and_numeric_documents_fail() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    for path in [
        "docs/dump.md",
        "src/features/run/docs/guides/01_guide.md",
        "src/features/run/docs/guides/_guide.md",
        "src/features/run/docs/guides/save_guide.MD",
        "src/features/run/docs/guides/nested-topic/save_guide.md",
        "tools/README.md",
    ] {
        write(root.path(), path, "");
    }
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(report.counts["architecture/feature-layout"], 2);
    assert_eq!(report.counts["architecture/document-role"], 2);
    assert_eq!(report.counts["architecture/role-filename"], 3);
    assert_eq!(report.counts["architecture/file-name"], 4);
    assert!(report.diagnostics.iter().filter(|d| d.rule == "architecture/document-role")
        .all(|d| d.expected.contains("_verification.md") && d.expected.contains("_standard.md")));
}

#[test]
fn native_names_and_reasoned_exclusions_are_narrow_and_optional() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("vendor")).unwrap();
    let project = project(
        root.path(),
        "[[excluded]]\npath='vendor'\nreason='Upstream'\n[[documentation_exclusions]]\nincludes=['generated/**', 'legal/ORIGINAL-LICENSE.md']\nreason='Generated review material and exact original notice'\n",
    );
    for path in [
        "README.md",
        "src/features/run/AGENTS.md",
        "legal/LICENSE.md",
        "legal/COPYING.md",
        "legal/NOTICE.md",
        "legal/ORIGINAL-LICENSE.md",
        "vendor/bad.md",
        "generated/deep/README.md",
    ] {
        write(root.path(), path, "");
    }
    assert_eq!(analyze_project(&project, &[]).unwrap().errors, 0);
    write(root.path(), "src/features/run/README.md", "");
    write(root.path(), "generated/bad.gd", "extends Node\n");
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(report.counts["architecture/document-role"], 1);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.path == "generated/bad.gd")
    );
}

#[test]
fn exclusions_reject_unsafe_patterns_missing_reasons_and_unknown_fields() {
    for fields in [
        "includes=[]\nreason='why'",
        "includes=['../docs/**']\nreason='why'",
        "includes=['/docs/**']\nreason='why'",
        "includes=['docs/**']\nreason=' '",
        "includes=['docs/**']\nreason='why'\nunknown=true",
        "includes=['docs/[ab]']\nreason='why'",
    ] {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "project.godot", "config_version=5\n");
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        write(
            root.path(),
            "llm-lint.godot.toml",
            &format!("version=1\n[[documentation_exclusions]]\n{fields}\n"),
        );
        assert!(discover_project(root.path(), None).is_err(), "{fields}");
    }
}

#[test]
fn existing_and_shared_destinations_are_collisions() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    for path in [
        "src/features/run/docs/guides/Save.md",
        "src/features/run/docs/guides/save_guide.md",
        "src/features/run/docs/guides/Resume-Run.md",
        "src/features/run/docs/guides/resume_run.md",
    ] {
        write(root.path(), path, "");
    }
    let report = analyze_project(&project, &[]).unwrap();
    assert_eq!(report.counts["architecture/document-collision"], 3);
}

#[test]
fn targets_levels_and_exact_exceptions_apply_to_documents() {
    let root = tempfile::tempdir().unwrap();
    let project = project(
        root.path(),
        "[[overrides]]\nincludes=['src/features/run/**']\n[overrides.rules]\n'architecture/role-filename'='warn'\n[[exceptions]]\nrule='architecture/document-role'\npath='src/features/run/docs/legacy.md'\nitem='<document>'\nreason='Exact external contract'\n",
    );
    let path = "src/features/run/docs/guides/save.md";
    write(root.path(), path, &valid_document("guide"));
    review_documents(root.path(), &[path.into()]);
    write(root.path(), "src/features/run/docs/legacy.md", "");
    write(root.path(), "docs/unrelated.md", "");
    let report = analyze_project(&project, &[path.into()]).unwrap();
    assert_eq!(report.errors, 0, "{:?}", report.diagnostics);
    assert_eq!(report.warnings, 1);
    assert_eq!(report.diagnostics[0].path, path);
}

#[test]
fn cli_checks_report_document_errors_and_leave_markdown_untouched() {
    let root = tempfile::tempdir().unwrap();
    project(root.path(), "[gdstyle]\nauto=true\nversion='0.3.1'\n");
    let path = "src/features/run/docs/guides/save.md";
    let original = "# Save\n\nres://missing.tres\n```gdscript\ninvalid syntax\n```\n";
    write(root.path(), path, original);
    for command in ["architecture", "lint", "check"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"))
            .current_dir(root.path())
            .args([command, path])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("architecture/role-filename"));
    }
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"))
        .current_dir(root.path())
        .args(["architecture", path, "--json"])
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["errors"].as_u64().unwrap() > 0);
    assert!(
        report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| {
                item["rule"] == "architecture/role-filename" && item["item"] == "<document>"
            })
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join(path)).unwrap(),
        original
    );
}

fn valid_document(role: &str) -> String {
    let sections = match role {
        "specification" => {
            "## Requirements\n\n- Preserve saves.\n\n## Acceptance Criteria\n\n- Resume the saved run.\n"
        }
        "guide" => {
            "## Prerequisites\n\n- Open the project.\n\n## Steps\n\n1. Resume the run.\n\n## Validation\n\n- Confirm the saved run resumes.\n"
        }
        "reference" => {
            "## Entries\n\n| Item | Meaning |\n| --- | --- |\n| Save | Stored checkpoint. |\n"
        }
        "standard" => {
            "## Rules\n\n- Preserve saves.\n\n## Validation\n\n- Confirm the saved run survives.\n"
        }
        "decision" => {
            "## Decision\n\n- Preserve saves.\n\n## Rationale\n\n- Resume interrupted play.\n\n## Alternatives\n\n- Restarting was rejected.\n\n## Consequences\n\n- Save compatibility is required.\n"
        }
        "verification" => {
            "## Results\n\n| Date | Check | Result | Evidence |\n| --- | --- | --- | --- |\n| 2026-01-01 | Save | Passed | Test log. |\n\n## Limitations\n\n- Device acceptance is outstanding.\n"
        }
        _ => panic!("Unknown test role"),
    };
    format!(
        "+++\nreviewed = \"2026-01-01\"\n+++\n# Content\n\n## Summary\n\nInvalid syntax `load(\"res://missing.tres\")` is documentation, not runtime source.\n\n## Key Files\n\n| Path | Purpose |\n| --- | --- |\n| `project.godot` | Project settings. |\n\n{sections}\n## Related Documents\n\n- [Project](README.md).\n"
    )
}

fn review_documents(root: &std::path::Path, paths: &[String]) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"))
        .current_dir(root)
        .args(["docs", "review"])
        .args(paths)
        .args(["--reviewed", "2026-01-01", "--config"])
        .arg(root.join("llm-lint.godot.toml"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
