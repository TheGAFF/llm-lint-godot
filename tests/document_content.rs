#![cfg(test)]
mod support;
use godot_llm_lint::{
    features::{
        architecture::helpers::{analyze_project, corpus},
        configuration::{helpers::discover_project, models::Project},
        duplicates::helpers::analyze_duplicates,
    },
    shared::models::ArchitectureReport,
};
use std::{fs, path::Path, process::Command};
use support::{project, write};

const GUIDE: &str = "src/features/run/docs/guides/checkpoint_guide.md";
const DECISION: &str = "src/features/run/docs/decisions/checkpoint_decision.md";
const RECORD: &str = ".llm-lint/doc-reviews/src/features/run/docs/guides/checkpoint_guide.md.json";

fn role_sections(role: &str) -> &'static str {
    match role {
        "specification" => {
            "## Requirements\n\n- Save a checkpoint.\n\n## Acceptance Criteria\n\n- Resume preserves the checkpoint.\n"
        }
        "guide" => {
            "## Prerequisites\n\n- A configured project.\n\n## Steps\n\n1. Open the checkpoint.\n2. Resume the run.\n\n## Validation\n\n- Confirm the checkpoint resumes.\n"
        }
        "reference" => {
            "## Entries\n\n| Field | Meaning |\n| --- | --- |\n| Checkpoint | The saved run. |\n"
        }
        "standard" => {
            "## Rules\n\n- Preserve the checkpoint.\n\n## Validation\n\n- Confirm the saved run survives.\n"
        }
        "decision" => {
            "## Decision\n\n- 2026-01-01: preserve checkpoints.\n\n## Rationale\n\n- Players resume interrupted runs.\n\n## Alternatives\n\n- Restarting was rejected.\n\n## Consequences\n\n- Saves require compatibility checks.\n"
        }
        "verification" => {
            "## Results\n\n| Date | Check | Result | Evidence |\n| --- | --- | --- | --- |\n| 2026-01-01 | Save test | Passed | Recorded test output. |\n\n## Limitations\n\n- Physical device acceptance was not performed.\n"
        }
        _ => panic!("Unknown fixture role: {role}"),
    }
}

fn document(role: &str, source: &str) -> String {
    format!(
        "+++\nreviewed = \"2026-01-01\"\n+++\n# Checkpoint\n\n## Summary\n\nCheckpoint behavior and its review evidence.\n\n## Key Files\n\n| Path | Purpose |\n| --- | --- |\n| `{source}` | Checkpoint implementation. |\n\n{}\n## Related Documents\n\n- [Project introduction](README.md).\n",
        role_sections(role)
    )
}

fn command(root: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"))
        .current_dir(root)
        .args(arguments)
        .arg("--config")
        .arg(root.join("llm-lint.godot.toml"))
        .output()
        .unwrap()
}

fn review(root: &Path, paths: &[&str]) {
    let mut arguments = vec!["docs", "review"];
    arguments.extend_from_slice(paths);
    arguments.extend(["--reviewed", "2026-01-01"]);
    let output = command(root, &arguments);
    assert!(
        output.status.success(),
        "Review failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn report(project: &Project) -> ArchitectureReport {
    analyze_project(project, &[]).unwrap()
}

fn has(report: &ArchitectureReport, suffix: &str) -> bool {
    report
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.rule == format!("architecture/document-{suffix}"))
}

fn source_details(report: &ArchitectureReport, suffix: &str) -> serde_json::Value {
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.path == GUIDE && diagnostic.rule == format!("architecture/document-{suffix}")
        })
        .unwrap();
    serde_json::to_value(diagnostic.document_sources.as_ref().unwrap()).unwrap()
}

fn assert_clean(project: &Project) {
    let report = report(project);
    assert_eq!(report.errors, 0, "{:?}", report.diagnostics);
}

fn body(text: &str) -> &str {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.starts_with("+++\r\n") {
        return text.splitn(3, "+++\r\n").nth(2).unwrap();
    }
    text.splitn(3, "+++\n").nth(2).unwrap()
}

#[test]
fn all_six_blueprints_accept_valid_documents() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    let mut paths = Vec::new();
    for (folder, role) in [
        ("specifications", "specification"),
        ("guides", "guide"),
        ("references", "reference"),
        ("standards", "standard"),
        ("decisions", "decision"),
        ("verification", "verification"),
    ] {
        let path = format!("src/features/run/docs/{folder}/checkpoint_{role}.md");
        write(root.path(), &path, &document(role, "project.godot"));
        paths.push(path);
    }
    review(
        root.path(),
        &paths.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    assert_clean(&project);
}

#[test]
fn content_rules_are_errors_without_opt_in_and_checks_are_read_only() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    let original = "# Legacy documentation\n\nUnstructured prose.\n";
    write(root.path(), GUIDE, original);
    let report = report(&project);
    for suffix in ["metadata", "blueprint", "sources"] {
        assert!(has(&report, suffix), "{:?}", report.diagnostics);
    }
    assert!(
        report
            .diagnostics
            .iter()
            .all(|item| item.severity == "error")
    );
    assert_eq!(
        fs::read_to_string(root.path().join(GUIDE)).unwrap(),
        original
    );
}

#[test]
fn metadata_rejects_invalid_dates_duplicates_unknowns_and_wrong_types() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    let original = document("decision", "historical/missing.gd");
    for metadata in [
        "reviewed = \"2026-02-29\"",
        "reviewed = \"1900-02-29\"",
        "reviewed = \"2026-13-01\"",
        "reviewed = \"2026-01-32\"",
        "reviewed = \"9999-01-01\"",
        "reviewed = \"2026-1-1\"",
        "reviewed = 2026-01-01",
        "reviewed = 7",
        "reviewed = \"2026-01-01\"\nreviewed = \"2026-01-02\"",
        "reviewed = \"2026-01-01\"\nunknown = true",
        "reviewed = \"2026-01-01\"\nsources_hash = \"sha256:ABC\"",
    ] {
        let text = format!("+++\n{metadata}\n+++\n{}", body(&original));
        write(root.path(), DECISION, &text);
        let report = report(&project);
        assert!(
            has(&report, "metadata"),
            "{metadata}: {:?}",
            report.diagnostics
        );
        assert!(
            report
                .diagnostics
                .iter()
                .all(|item| item.line > 0 && item.column > 0)
        );
    }
    write(
        root.path(),
        DECISION,
        &original.replace("2026-01-01\"", "2000-02-29\""),
    );
    assert_clean(&project);
}

#[test]
fn living_roles_require_a_reviewed_source_fingerprint() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(root.path(), GUIDE, &document("guide", "project.godot"));
    assert!(has(&report(&project), "metadata"));
    review(root.path(), &[GUIDE]);
    assert_clean(&project);
    let text = fs::read_to_string(root.path().join(GUIDE)).unwrap();
    assert!(text.contains("sources_hash = \"sha256:"));
}

#[test]
fn blueprint_rejects_missing_reordered_duplicate_and_deep_headings() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    let original = document("decision", "historical/missing.gd");
    for text in [
        original.replace("## Rationale", "## Explanation"),
        original
            .replace("## Decision", "## Rationale")
            .replacen("## Rationale", "## Decision", 2),
        original.replace(
            "## Consequences",
            "## Decision\n\n- Duplicate.\n\n## Consequences",
        ),
        original.replace("## Rationale", "#### Rationale"),
        original.replace("# Checkpoint", "# Checkpoint\n\n# Another title"),
        original.replace("## Summary", "### Summary"),
    ] {
        write(root.path(), DECISION, &text);
        let report = report(&project);
        assert!(has(&report, "blueprint"), "{:?}", report.diagnostics);
    }
}

#[test]
fn structures_reject_wrong_list_forms_tables_empty_sections_and_long_summary() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    let original = document("guide", "project.godot");
    for text in [
        original.replace(
            "1. Open the checkpoint.\n2. Resume the run.",
            "- Open the checkpoint.\n- Resume the run.",
        ),
        original.replace("- A configured project.", "1. A configured project."),
        original.replace("| Path | Purpose |", "| File | Why |"),
        original.replace("- Confirm the checkpoint resumes.", ""),
        original.replace(
            "- [Project introduction](README.md).",
            "- Unlinked project introduction.",
        ),
        original.replace(
            "Checkpoint behavior and its review evidence.",
            &"word ".repeat(81),
        ),
        original.replace(
            "Checkpoint behavior and its review evidence.",
            "First paragraph.\n\nSecond paragraph.",
        ),
    ] {
        write(root.path(), GUIDE, &text);
        let report = report(&project);
        assert!(
            has(&report, "structure") || has(&report, "sources"),
            "{:?}",
            report.diagnostics
        );
    }
}

#[test]
fn role_tables_require_nonempty_headers_and_verification_columns() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    let path = "src/features/run/docs/verification/checkpoint_verification.md";
    let original = document("verification", "historical/missing.gd");
    write(
        root.path(),
        path,
        &original.replace(
            "| Date | Check | Result | Evidence |",
            "| Date | Check | Result | Notes |",
        ),
    );
    assert!(has(&report(&project), "structure"));
    fs::remove_file(root.path().join(path)).unwrap();
    let path = "src/features/run/docs/references/checkpoint_reference.md";
    write(
        root.path(),
        path,
        &document("reference", "project.godot").replace("| Field | Meaning |", "| Field |  |"),
    );
    assert!(has(&report(&project), "structure"));
}

#[test]
fn grouped_lists_and_attached_fences_do_not_enter_runtime_analysis() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    let path = "src/features/run/docs/specifications/checkpoint_specification.md";
    let text = document("specification", "project.godot").replace(
        "- Save a checkpoint.",
        "### Saved run\n\n- Save a checkpoint.\n  - Preserve its identifier.\n\n  ```gdscript\n  # A fake title\n  ## Summary\n  invalid syntax load(\"res://missing.tres\")\n  # llm-lint-ignore nonsense\n  ```",
    );
    write(root.path(), path, &text);
    review(root.path(), &[path]);
    assert_clean(&project);
    let corpus = corpus(&project).unwrap();
    assert_eq!(corpus.documents.len(), 1);
    assert!(corpus.documents[0].references.is_empty());
    assert!(corpus.documents[0].functions.is_empty());
    assert_eq!(
        analyze_duplicates(&project, &[])
            .unwrap()
            .summary
            .analyzed_units,
        0
    );
    let too_deep = text.replace(
        "  - Preserve its identifier.",
        "  - Preserve its identifier.\n    - Too deeply nested.",
    );
    write(root.path(), path, &too_deep);
    assert!(has(&report(&project), "structure"));
}

#[test]
fn utf8_bom_crlf_and_non_ascii_prose_preserve_body_bytes_during_review() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    let original = format!(
        "\u{feff}{}",
        document("guide", "project.godot")
            .replace("Checkpoint behavior", "チェックポイント behavior")
            .replace('\n', "\r\n")
    );
    write(root.path(), GUIDE, &original);
    review(root.path(), &[GUIDE]);
    let reviewed = fs::read_to_string(root.path().join(GUIDE)).unwrap();
    assert_eq!(body(&reviewed).as_bytes(), body(&original).as_bytes());
    assert_clean(&project);
}

#[test]
fn source_edits_trigger_same_day_review_without_git_or_lint_writes() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(root.path(), "inputs/options.json", "{\"save\":true}\n");
    write(
        root.path(),
        GUIDE,
        &document("guide", "inputs/options.json"),
    );
    review(root.path(), &[GUIDE]);
    assert_clean(&project);
    assert!(!root.path().join(".git").exists());
    let reviewed = fs::read_to_string(root.path().join(GUIDE)).unwrap();
    write(root.path(), "inputs/options.json", "{\"save\":false}\n");
    let report = report(&project);
    assert!(has(&report, "stale"));
    let details = source_details(&report, "stale");
    assert_eq!(details["baseline"], "review_record");
    assert_eq!(
        details["modified"],
        serde_json::json!(["inputs/options.json"])
    );
    let record = fs::read(root.path().join(RECORD)).unwrap();
    let terminal = command(root.path(), &["architecture", GUIDE]);
    assert!(String::from_utf8_lossy(&terminal.stderr).contains("Modified: inputs/options.json"));
    assert_eq!(
        fs::read_to_string(root.path().join(GUIDE)).unwrap(),
        reviewed
    );
    assert_eq!(fs::read(root.path().join(RECORD)).unwrap(), record);
    review(root.path(), &[GUIDE]);
    assert_clean(&project);
}

#[test]
fn source_globs_detect_additions_removals_renames_and_empty_matches() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(root.path(), "inputs/first.txt", "first\n");
    write(root.path(), GUIDE, &document("guide", "inputs/*.txt"));
    review(root.path(), &[GUIDE]);
    write(root.path(), "inputs/second.txt", "second\n");
    assert_eq!(
        source_details(&report(&project), "stale")["added"],
        serde_json::json!(["inputs/second.txt"])
    );
    review(root.path(), &[GUIDE]);
    fs::remove_file(root.path().join("inputs/first.txt")).unwrap();
    assert_eq!(
        source_details(&report(&project), "stale")["removed"],
        serde_json::json!(["inputs/first.txt"])
    );
    review(root.path(), &[GUIDE]);
    // A rename is a path-set change with identical bytes. Copy/remove also works
    // in Windows app sandboxes that deny the native rename syscall.
    fs::copy(
        root.path().join("inputs/second.txt"),
        root.path().join("inputs/renamed.txt"),
    )
    .unwrap();
    fs::remove_file(root.path().join("inputs/second.txt")).unwrap();
    let rename = source_details(&report(&project), "stale");
    assert_eq!(rename["removed"], serde_json::json!(["inputs/second.txt"]));
    assert_eq!(rename["added"], serde_json::json!(["inputs/renamed.txt"]));
    review(root.path(), &[GUIDE]);
    fs::remove_file(root.path().join("inputs/renamed.txt")).unwrap();
    assert_eq!(
        source_details(&report(&project), "sources")["removed"],
        serde_json::json!(["inputs/renamed.txt"])
    );
}

#[test]
fn source_paths_are_safe_case_sensitive_nonempty_and_not_self_referential() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    for source in [
        "../outside.txt",
        "/absolute.txt",
        "C:/absolute.txt",
        "inputs\\data.txt",
        "inputs/./data.txt",
        "inputs/../data.txt",
        "inputs/[ab].txt",
        "Project.godot",
        "inputs/missing.txt",
        "inputs/*.missing",
        GUIDE,
    ] {
        write(root.path(), GUIDE, &document("guide", source));
        assert!(has(&report(&project), "sources"), "{source}");
    }
    let original = document("guide", "project.godot");
    write(
        root.path(),
        GUIDE,
        &original.replace("Checkpoint implementation.", ""),
    );
    assert!(has(&report(&project), "sources") || has(&report(&project), "structure"));
}

#[test]
fn companions_are_outside_source_globs_and_rejected_batches_write_nothing() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    let guide = document("guide", "**/*.json");
    write(root.path(), GUIDE, &guide);
    let rejected = "src/features/other/docs/guides/rejected_guide.md";
    write(root.path(), rejected, "# Rejected\n");
    write(root.path(), "inputs/seed.json", "{}\n");
    let result = command(
        root.path(),
        &[
            "docs",
            "review",
            GUIDE,
            rejected,
            "--reviewed",
            "2026-01-01",
        ],
    );
    assert_eq!(result.status.code(), Some(2));
    assert_eq!(fs::read_to_string(root.path().join(GUIDE)).unwrap(), guide);
    assert!(!root.path().join(".llm-lint").exists());
    review(root.path(), &[GUIDE]);
    let path = root.path().join(RECORD);
    let record = fs::read(&path).unwrap();
    let data: serde_json::Value = serde_json::from_slice(&record).unwrap();
    assert_eq!(data["files"].as_object().unwrap().len(), 1);
    let inventory = corpus(&project).unwrap();
    assert!(!inventory.paths.contains(RECORD));
    let check = analyze_project(&project, &[GUIDE.into()]).unwrap();
    assert_eq!(check.errors, 0, "{:?}", check.diagnostics);
    review(root.path(), &[GUIDE]);
    assert_eq!(fs::read(path).unwrap(), record);
}

#[test]
fn invalid_companions_never_claim_source_attribution() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(root.path(), GUIDE, &document("guide", "inputs/seed.txt"));
    write(root.path(), "inputs/seed.txt", "reviewed\n");
    review(root.path(), &[GUIDE]);
    let record = root.path().join(RECORD);
    let valid: serde_json::Value = serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
    write(root.path(), "inputs/seed.txt", "changed\n");
    let mut inconsistent = valid;
    inconsistent["files"]["inputs/seed.txt"] = serde_json::json!("a".repeat(64));
    for bytes in [
        b"bad json".to_vec(),
        serde_json::to_vec(&inconsistent).unwrap(),
    ] {
        fs::write(&record, bytes).unwrap();
        let details = source_details(&report(&project), "stale");
        assert_eq!(details["baseline"], "unavailable");
        assert!(details.get("modified").is_none());
    }
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{} {:?}",
        String::from_utf8_lossy(&output.stderr),
        args
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn legacy_git_baseline_must_reproduce_the_reviewed_digest() {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "--quiet"]);
    let root = repo.path().join("nested game");
    fs::create_dir(&root).unwrap();
    let project = project(&root, "");
    write(&root, GUIDE, &document("guide", "inputs/*.txt"));
    write(&root, "inputs/seed.txt", "reviewed\n");
    review(&root, &[GUIDE]);
    let companion = fs::read(root.join(RECORD)).unwrap();
    fs::remove_file(root.join(RECORD)).unwrap();
    git(repo.path(), &["add", "--all"]);
    let commit = |message| {
        git(
            repo.path(),
            &[
                "-c",
                "user.name=Lint Test",
                "-c",
                "user.email=lint-test@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                message,
            ],
        );
    };
    commit("Review sources");
    let first = git(repo.path(), &["rev-parse", "HEAD"]);

    write(&root, "inputs/seed.txt", "changed\n");
    git(repo.path(), &["add", "--all"]);
    commit("Change sources");
    let report = report(&project);
    assert_eq!(source_details(&report, "stale")["baseline"], "unavailable");
    let baseline = |reference| {
        let out = command(
            &root,
            &["architecture", GUIDE, "--doc-baseline", reference, "--json"],
        );
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        json["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|finding| finding["rule"] == "architecture/document-stale")
            .unwrap()["document_sources"]
            .clone()
    };
    assert_eq!(baseline("HEAD")["baseline"], "unavailable");
    let from_git = baseline(&first);
    assert_eq!(from_git["baseline"], "git");
    assert_eq!(from_git["commit"], first);
    assert_eq!(from_git["modified"], serde_json::json!(["inputs/seed.txt"]));
    let invalid = command(
        &root,
        &[
            "architecture",
            "--doc-baseline",
            "no-such-revision",
            "--json",
        ],
    );
    assert_eq!(invalid.status.code(), Some(2));
    fs::write(root.join(RECORD), companion).unwrap();
    assert_eq!(baseline("HEAD")["baseline"], "review_record");
}

#[test]
fn declarations_and_binary_bytes_are_part_of_fingerprint() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    fs::create_dir_all(root.path().join("inputs")).unwrap();
    fs::write(root.path().join("inputs/seed.bin"), [0, 255, 1]).unwrap();
    write(root.path(), GUIDE, &document("guide", "inputs/seed.bin"));
    review(root.path(), &[GUIDE]);
    let original = fs::read_to_string(root.path().join(GUIDE)).unwrap();
    write(
        root.path(),
        GUIDE,
        &original.replace("Checkpoint implementation.", "Seed bytes."),
    );
    let wording = source_details(&report(&project), "stale");
    assert_eq!(wording["declarations_changed"], true);
    assert_eq!(wording["modified"], serde_json::json!([]));
    review(root.path(), &[GUIDE]);
    fs::write(root.path().join("inputs/seed.bin"), [0, 255, 2]).unwrap();
    assert!(has(&report(&project), "stale"));
}

#[test]
fn text_newlines_bom_and_related_markdown_review_metadata_do_not_cause_churn() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        DECISION,
        &document("decision", "historical/missing.gd"),
    );
    write(root.path(), GUIDE, &document("guide", DECISION));
    review(root.path(), &[GUIDE]);
    let snapshot = fs::read_to_string(root.path().join(DECISION)).unwrap();
    write(
        root.path(),
        DECISION,
        &format!(
            "\u{feff}{}",
            snapshot
                .replace("reviewed = \"2026-01-01\"", "reviewed = \"2026-01-02\"")
                .replace('\n', "\r\n")
        ),
    );
    assert_clean(&project);
    write(
        root.path(),
        DECISION,
        &snapshot.replace(
            "Players resume interrupted runs.",
            "Players resume interrupted runs safely.",
        ),
    );
    assert!(has(&report(&project), "stale"));
}

#[test]
fn wildcard_markdown_dependencies_omit_self_without_hash_cycles() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(
        root.path(),
        DECISION,
        &document("decision", "historical/missing.gd"),
    );
    write(
        root.path(),
        GUIDE,
        &document("guide", "src/features/run/docs/**/*.md"),
    );
    review(root.path(), &[GUIDE]);
    assert_clean(&project);
    review(root.path(), &[GUIDE]);
    assert_clean(&project);
}

#[test]
fn targeted_nonruntime_files_include_cross_owner_dependents_and_not_unrelated_docs() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("tools")).unwrap();
    let project = project(root.path(), "tooling_roots=['tools']\n");
    write(root.path(), "tools/export.py", "print('first')\n");
    write(root.path(), GUIDE, &document("guide", "tools/*.py"));
    let unrelated = "src/features/other/docs/guides/other_guide.md";
    write(
        root.path(),
        unrelated,
        &document("guide", "inputs/unrelated.txt"),
    );
    write(root.path(), "inputs/unrelated.txt", "first\n");
    review(root.path(), &[GUIDE, unrelated]);
    write(root.path(), "tools/export.py", "print('second')\n");
    write(root.path(), "inputs/unrelated.txt", "second\n");
    for target in ["tools/export.py", "tools"] {
        let report = analyze_project(&project, &[target.into()]).unwrap();
        assert!(has(&report, "stale"), "{:?}", report.diagnostics);
        assert!(report.diagnostics.iter().any(|item| item.path == GUIDE));
        assert!(!report.diagnostics.iter().any(|item| item.path == unrelated));
    }
}

#[test]
fn removed_exact_source_is_reported_from_existing_parent_target() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    write(root.path(), "inputs/source.txt", "first\n");
    write(root.path(), GUIDE, &document("guide", "inputs/source.txt"));
    review(root.path(), &[GUIDE]);
    fs::remove_file(root.path().join("inputs/source.txt")).unwrap();
    let report = analyze_project(&project, &["inputs".into()]).unwrap();
    assert!(has(&report, "sources"), "{:?}", report.diagnostics);
    assert!(report.diagnostics.iter().any(|item| item.path == GUIDE));
    assert_eq!(
        source_details(&report, "sources")["removed"],
        serde_json::json!(["inputs/source.txt"])
    );
}

#[test]
fn removed_last_glob_source_is_reported_from_existing_parent_target() {
    let root = tempfile::tempdir().unwrap();
    project(root.path(), "");
    write(root.path(), "inputs/nested/source.txt", "first\n");
    write(root.path(), GUIDE, &document("guide", "inputs/**/*.txt"));
    review(root.path(), &[GUIDE]);
    fs::remove_file(root.path().join("inputs/nested/source.txt")).unwrap();

    let result = command(root.path(), &["architecture", "inputs", "--json"]);
    assert_eq!(result.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(
        report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| {
                item["path"] == GUIDE && item["rule"] == "architecture/document-sources"
            })
    );
}

#[test]
fn historical_snapshots_preserve_original_evidence_and_do_not_track_current_sources() {
    let root = tempfile::tempdir().unwrap();
    let project = project(root.path(), "");
    let snapshot = document("decision", "historical/missing.gd").replace(
        "reviewed = \"2026-01-01\"",
        &format!(
            "reviewed = \"2026-01-01\"\nsources_hash = \"sha256:{}\"",
            "a".repeat(64)
        ),
    );
    write(root.path(), DECISION, &snapshot);
    assert_clean(&project);
    write(
        root.path(),
        "historical/missing.gd",
        "# Historical file, recreated elsewhere.\n",
    );
    fs::remove_file(root.path().join("historical/missing.gd")).unwrap();
    let output = command(
        root.path(),
        &["docs", "review", DECISION, "--reviewed", "2026-01-02"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reviewed = fs::read_to_string(root.path().join(DECISION)).unwrap();
    assert!(
        !root
            .path()
            .join(format!(".llm-lint/doc-reviews/{DECISION}.json"))
            .exists()
    );
    assert!(reviewed.contains(&format!("sources_hash = \"sha256:{}\"", "a".repeat(64))));
    assert_eq!(body(&reviewed), body(&snapshot));
    assert!(reviewed.contains("2026-01-01: preserve checkpoints."));
    assert_clean(&project);
}

#[test]
fn line_budget_counts_title_blank_lines_tables_and_code_but_not_frontmatter() {
    let root = tempfile::tempdir().unwrap();
    let original = document("decision", "historical/missing.gd");
    let lines = body(&original).lines().count();
    let config = format!(
        "[rules.'architecture/document-size']\noptions={{max_lines={lines},max_words=1000}}\n"
    );
    let project = project(root.path(), &config);
    write(root.path(), DECISION, &original);
    assert_clean(&project);
    write(root.path(), DECISION, &format!("{original}\n"));
    let report = report(&project);
    assert!(has(&report, "size"), "{:?}", report.diagnostics);
    let issue = report
        .diagnostics
        .iter()
        .find(|item| item.rule == "architecture/document-size")
        .unwrap();
    assert!(issue.actual.contains(&(lines + 1).to_string()));
    assert!(issue.expected.contains(&lines.to_string()));
}

#[test]
fn word_budget_cannot_be_bypassed_with_a_single_dense_line() {
    let root = tempfile::tempdir().unwrap();
    let project = project(
        root.path(),
        "[rules.'architecture/document-size']\noptions={max_lines=1000,max_words=100}\n",
    );
    let original = document("decision", "historical/missing.gd");
    write(root.path(), DECISION, &original);
    assert_clean(&project);
    write(
        root.path(),
        DECISION,
        &original.replace(
            "- Players resume interrupted runs.",
            &format!("- {}", "word ".repeat(101)),
        ),
    );
    let report = report(&project);
    assert!(has(&report, "size"));
    assert!(!has(&report, "structure"));
}

#[test]
fn exact_word_budget_accepts_the_boundary_and_rejects_one_extra_word() {
    let root = tempfile::tempdir().unwrap();
    let project = project(
        root.path(),
        "[rules.'architecture/document-size']\noptions={max_lines=1000,max_words=37}\n",
    );
    let original = document("decision", "historical/missing.gd");
    write(root.path(), DECISION, &original);
    assert_clean(&project);
    write(
        root.path(),
        DECISION,
        &original.replace(
            "Players resume interrupted runs.",
            "Players resume interrupted runs safely.",
        ),
    );
    let report = report(&project);
    let issue = report
        .diagnostics
        .iter()
        .find(|item| item.rule == "architecture/document-size")
        .unwrap();
    assert!(
        issue.actual.contains("38 words"),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn content_rule_levels_path_overrides_and_exact_exceptions_remain_supported() {
    let root = tempfile::tempdir().unwrap();
    let original = document("decision", "historical/missing.gd");
    let invalid = original.replace("## Rationale", "## Explanation");
    let global = "[rules]\n'architecture/document-blueprint'='off'\n";
    let settings = project(root.path(), global);
    write(root.path(), DECISION, &invalid);
    assert_clean(&settings);
    let settings = project(
        root.path(),
        &format!(
            "{global}[[overrides]]\nincludes=['src/features/run/**']\n[overrides.rules]\n'architecture/document-blueprint'='warn'\n"
        ),
    );
    let warning = report(&settings);
    assert_eq!(warning.errors, 0);
    assert_eq!(warning.warnings, 1);
    assert_eq!(
        warning.diagnostics[0].rule,
        "architecture/document-blueprint"
    );
    let settings = project(
        root.path(),
        &format!(
            "[[exceptions]]\nrule='architecture/document-blueprint'\npath='{DECISION}'\nitem='<document>'\nreason='Retained external heading contract'\n"
        ),
    );
    assert_clean(&settings);
    write(root.path(), DECISION, &original);
    assert!(
        report(&settings)
            .diagnostics
            .iter()
            .any(|item| item.rule == "architecture/exception-stale")
    );
}

#[test]
fn size_options_require_known_positive_integer_thresholds() {
    let root = tempfile::tempdir().unwrap();
    project(root.path(), "");
    for options in [
        "{max_lines=0}",
        "{max_words=0}",
        "{max_words=-1}",
        "{max_words='100'}",
        "{max_characters=100}",
    ] {
        write(
            root.path(),
            "llm-lint.godot.toml",
            &format!("version=1\n[rules.'architecture/document-size']\noptions={options}\n"),
        );
        assert!(discover_project(root.path(), None).is_err(), "{options}");
    }
}

#[test]
fn review_validates_all_requested_documents_before_any_metadata_writes() {
    let root = tempfile::tempdir().unwrap();
    project(root.path(), "");
    let valid = document("guide", "project.godot");
    let invalid = document("decision", "historical/missing.gd").replace("## Rationale", "## Wrong");
    write(root.path(), GUIDE, &valid);
    write(root.path(), DECISION, &invalid);
    let output = command(
        root.path(),
        &[
            "docs",
            "review",
            GUIDE,
            DECISION,
            "--reviewed",
            "2026-01-02",
        ],
    );
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(root.path().join(GUIDE)).unwrap(), valid);
    assert_eq!(
        fs::read_to_string(root.path().join(DECISION)).unwrap(),
        invalid
    );
}

#[test]
fn review_rejects_bad_dates_unknown_metadata_unsafe_sources_and_nonexplicit_targets() {
    let root = tempfile::tempdir().unwrap();
    project(root.path(), "");
    let valid = document("guide", "project.godot");
    write(root.path(), GUIDE, &valid);
    for arguments in [
        vec!["docs", "review", GUIDE, "--reviewed", "9999-01-01"],
        vec![
            "docs",
            "review",
            "src/features/run/docs",
            "--reviewed",
            "2026-01-01",
        ],
        vec!["docs", "review", "src/**/*.md", "--reviewed", "2026-01-01"],
        vec!["docs", "review", GUIDE],
    ] {
        assert!(!command(root.path(), &arguments).status.success());
        assert_eq!(fs::read_to_string(root.path().join(GUIDE)).unwrap(), valid);
    }
    for invalid in [
        valid.replace("reviewed = \"2026-01-01\"", "unknown = true"),
        valid.replace("reviewed = \"2026-01-01\"", "reviewed = 7"),
        valid.replace(
            "reviewed = \"2026-01-01\"",
            "reviewed = \"2026-01-01\"\nsources_hash = 7",
        ),
        valid.replace("`project.godot`", "`../outside.txt`"),
    ] {
        write(root.path(), GUIDE, &invalid);
        assert!(
            !command(
                root.path(),
                &["docs", "review", GUIDE, "--reviewed", "2026-01-01"]
            )
            .status
            .success()
        );
        assert_eq!(
            fs::read_to_string(root.path().join(GUIDE)).unwrap(),
            invalid
        );
    }
}

#[test]
fn architecture_json_reports_document_locations_deterministically_without_writes() {
    let root = tempfile::tempdir().unwrap();
    project(root.path(), "");
    let original =
        document("decision", "historical/missing.gd").replace("## Rationale", "## Wrong");
    write(root.path(), DECISION, &original);
    let first = command(root.path(), &["architecture", DECISION, "--json"]);
    let second = command(root.path(), &["architecture", DECISION, "--json"]);
    assert_eq!(first.status.code(), Some(1));
    assert_eq!(first.stdout, second.stdout);
    let report: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert!(report["errors"].as_u64().unwrap() > 0);
    assert!(
        report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["path"] == DECISION && item["line"].as_u64().unwrap() > 0)
    );
    assert_eq!(
        fs::read_to_string(root.path().join(DECISION)).unwrap(),
        original
    );
}
