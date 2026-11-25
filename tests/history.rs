#![cfg(test)]
mod support;

use rusqlite::Connection;
use std::{
    fs,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Output, Stdio},
};
use support::{project, write};

const HISTORY: &str = ".tools/godot-llm-lint/history.sqlite3";
const SCRIPT: &str = "src/shared/queries/value_query.gd";
const CLEAN: &str = "extends RefCounted\n\n\nstatic func get_value() -> int:\n\treturn 1\n";
const TYPE_HINT: &str = "extends RefCounted\n\n\nstatic func get_value():\n\treturn 1\n";

fn fixture(settings: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    project(
        root.path(),
        &format!("[gdstyle]\nauto=true\nversion='0.8.0'\n{settings}"),
    );
    root
}

fn cli(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"));
    command.current_dir(root).args(args);
    command
}

fn invoke(root: &Path, args: &[&str], code: i32) -> Output {
    let output = cli(root, args).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn record(root: &Path, model: &str, baseline: Option<&str>, code: i32) -> Output {
    let mut args = vec!["check", "--record", "--model", model];
    if let Some(id) = baseline {
        args.extend(["--baseline-run", id]);
    }
    invoke(root, &args, code)
}

fn history(root: &Path) -> Connection {
    Connection::open(root.join(HISTORY)).unwrap()
}

fn scalar(database: &Connection, statement: &str) -> i64 {
    database.query_row(statement, [], |row| row.get(0)).unwrap()
}

#[test]
fn recording_is_opt_in_and_reports_linked_model_changes() {
    let fixture = fixture("");
    let root = fixture.path();
    write(
        root,
        SCRIPT,
        "class_name Wrong\nextends RefCounted\nstatic func get_value():\n    return 1\n",
    );
    invoke(root, &["check"], 1);
    invoke(root, &["check", "--record"], 2);
    invoke(root, &["check", "--record", "--model", " "], 2);
    invoke(root, &["check", "--record", "--model", "Agent", "src"], 2);
    invoke(root, &["lint", "--record", "--model", "Agent"], 2);
    invoke(root, &["history", "report"], 2);
    assert!(!root.join(HISTORY).exists());

    let baseline = invoke(root, &["check", "--record", "--model", "Starting point"], 1);
    assert!(String::from_utf8_lossy(&baseline.stderr).contains("Recorded lint run 1"));
    let database = history(root);
    for rule in [
        "architecture/class-name",
        "gdstyle/quality/type-hint",
        "formatting/check",
    ] {
        assert!(
            database
                .query_row(
                    "SELECT count(*) FROM findings WHERE rule_id = ?1 AND run_id = 1",
                    [rule],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap()
                > 0,
            "{rule}"
        );
    }

    write(root, SCRIPT, CLEAN);
    let model = "<script>alert(\"test\")</script>";
    invoke(
        root,
        &["check", "--record", "--model", model, "--baseline-run", "1"],
        0,
    );
    assert_eq!(
        scalar(&database, "SELECT baseline_run_id FROM runs WHERE id = 2"),
        1
    );
    let report = invoke(
        root,
        &[
            "history",
            "report",
            "--model",
            model,
            "--since",
            "2000-01-01",
        ],
        0,
    );
    let text = String::from_utf8(report.stdout).unwrap();
    assert!(text.contains("100.0%"));
    assert!(text.lines().any(|line| {
        line.split_whitespace().collect::<Vec<_>>()
            == [model, "architecture/class-name", "error", "1", "0"]
    }));
    assert!(!text.contains("Starting point"));
    let details = invoke(root, &["history", "report", "--run", "1"], 0);
    assert!(String::from_utf8_lossy(&details.stdout).contains(SCRIPT));
    let empty = invoke(root, &["history", "report", "--since", "9999-01-01"], 0);
    assert!(String::from_utf8_lossy(&empty.stdout).contains("No matching runs."));
    invoke(root, &["history", "report", "--since", "2026-02-30"], 2);

    invoke(root, &["history", "report", "--html", "report.html"], 0);
    let html = fs::read_to_string(root.join("report.html")).unwrap();
    assert!(html.contains("&lt;script&gt;alert(&quot;test&quot;)&lt;/script&gt;"));
    assert!(!html.contains("<script>"));
    assert!(html.contains("<details>") && html.contains(SCRIPT));
    invoke(root, &["history", "report", "--html", HISTORY], 2);
    database
        .execute("UPDATE runs SET linter_version = 'older' WHERE id = 1", [])
        .unwrap();
    record(root, "Agent", Some("1"), 2);
    invoke(
        root,
        &[
            "check",
            "--record",
            "--model",
            "Agent",
            "--baseline-run",
            "99",
        ],
        2,
    );
    assert_eq!(scalar(&database, "SELECT count(*) FROM runs"), 2);
}

#[test]
fn warnings_and_duplicate_locations_are_saved_and_incomplete_runs_are_excluded() {
    let fixture = fixture(
        "[duplicates]\nwarning_tokens=10\nwarning_lines=3\n[rules]\n'gdstyle/quality/type-hint'='warn'\n",
    );
    let root = fixture.path();
    write(root, SCRIPT, TYPE_HINT);
    let duplicate = "extends RefCounted\n\n\nstatic func calculate(input: int) -> int:\n\tvar total: int = input + 1\n\ttotal += 2\n\ttotal *= 3\n\treturn total\n";
    let left = "src/features/ui/testing/one_test_harness.gd";
    let right = "src/features/ui/testing/two_test_harness.gd";
    write(root, left, duplicate);
    write(root, right, duplicate);
    invoke(root, &["check", "--record", "--model", "Agent"], 0);
    let database = history(root);
    assert_eq!(
        scalar(
            &database,
            "SELECT count(*) FROM findings WHERE severity = 'error'"
        ),
        0
    );
    assert_eq!(
        scalar(
            &database,
            "SELECT count(*) FROM findings WHERE rule_id = 'gdstyle/quality/type-hint'"
        ),
        1
    );
    assert_eq!(
        scalar(
            &database,
            "SELECT count(*) FROM findings WHERE rule_id = 'duplicates/exact'"
        ),
        1
    );
    let locations: Vec<String> = database
        .prepare(
            "SELECT path FROM finding_locations JOIN findings ON findings.id = finding_id
         WHERE rule_id = 'duplicates/exact' ORDER BY ordinal",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(locations, [left, right]);

    let config = root.join("llm-lint.godot.toml");
    let source = fs::read_to_string(&config).unwrap();
    fs::write(
        &config,
        source.replace("warning_tokens=10", "warning_tokens=11"),
    )
    .unwrap();
    record(root, "Agent", Some("1"), 2);
    fs::write(config, source).unwrap();
    let empty_cache = tempfile::tempdir().unwrap();
    write(root, SCRIPT, &format!("class_name Wrong\n{CLEAN}"));
    let failed = cli(
        root,
        &[
            "check",
            "--record",
            "--model",
            "Agent",
            "--baseline-run",
            "1",
        ],
    )
    .env("GODOT_LLM_LINT_CACHE_DIR", empty_cache.path())
    .output()
    .unwrap();
    assert_eq!(failed.status.code(), Some(2));
    assert_eq!(
        scalar(
            &database,
            "SELECT exit_code FROM runs WHERE id = 2 AND failure_stage = 'native'"
        ),
        2
    );
    assert!(scalar(&database, "SELECT count(*) FROM findings WHERE run_id = 2") > 0);
    let report = invoke(root, &["history", "report"], 0);
    let text = String::from_utf8(report.stdout).unwrap();
    assert!(text.contains("Incomplete (native)") && text.contains("100.0%"));
    assert!(!text.contains("architecture/class-name"));
    record(root, "Agent", Some("2"), 2);
    assert_eq!(scalar(&database, "SELECT count(*) FROM runs"), 2);
}

#[test]
fn concurrent_writers_and_a_failed_insert_keep_complete_transactions() {
    let fixture = fixture("");
    let root = fixture.path();
    write(root, SCRIPT, CLEAN);
    let mut children = Vec::new();
    for model in ["One", "Two"] {
        children.push(
            cli(root, &["check", "--record", "--model", model])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let database = history(root);
    assert_eq!(scalar(&database, "SELECT count(*) FROM runs"), 2);
    database
        .execute_batch(
            "CREATE TRIGGER reject_location BEFORE INSERT ON finding_locations
         BEGIN SELECT RAISE(ABORT, 'test rollback'); END;",
        )
        .unwrap();
    write(root, SCRIPT, TYPE_HINT);
    let output = invoke(root, &["check", "--record", "--model", "Third"], 2);
    assert!(String::from_utf8_lossy(&output.stderr).contains("Could not record lint history"));
    assert_eq!(scalar(&database, "SELECT count(*) FROM runs"), 2);
    assert_eq!(scalar(&database, "SELECT count(*) FROM findings"), 0);
}

#[test]
fn terminating_a_waiting_writer_leaves_the_last_snapshot_intact() {
    let fixture = fixture("");
    let root = fixture.path();
    write(root, SCRIPT, CLEAN);
    invoke(root, &["check", "--record", "--model", "Saved"], 0);
    let database = history(root);
    database.execute_batch("BEGIN IMMEDIATE").unwrap();
    let mut child = cli(root, &["check", "--record", "--model", "Cancelled"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let reached_end_of_check = stdout
        .lines()
        .map(Result::unwrap)
        .any(|line| line.starts_with("Duplicates:"));
    let terminated = child.kill().is_ok();
    child.wait().unwrap();
    database.execute_batch("ROLLBACK").unwrap();
    assert!(reached_end_of_check && terminated);
    assert_eq!(scalar(&database, "SELECT count(*) FROM runs"), 1);
}
