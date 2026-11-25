mod read;

use super::{super::models::Capture, capture::timestamp};
use crate::shared::{models::Outcome, paths::guarded_path};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const APPLICATION_ID: i32 = 0x474c_4849;
const SCHEMA_VERSION: i32 = 1;
const LINTER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(super) fn history_path(root: &Path) -> Outcome<PathBuf> {
    const FILE: &str = ".tools/godot-llm-lint/history.sqlite3";
    for suffix in ["-journal", "-wal", "-shm"] {
        guarded_path(root, Path::new(&format!("{FILE}{suffix}")))?;
    }
    guarded_path(root, Path::new(FILE))
}

pub(super) fn baseline_status(path: &Path, id: i64, policy: &str) -> Outcome<()> {
    let database = existing(path)?;
    let matches = database
        .query_row(
            "SELECT exit_code IN (0, 1) AND linter_version = ?2 AND policy_fingerprint = ?3
             FROM runs WHERE id = ?1",
            params![id, LINTER_VERSION, policy],
            |row| row.get::<_, bool>(0),
        )
        .optional()?
        .unwrap_or(false);
    if !matches {
        return Err(format!(
            "Baseline run {id} must be a completed run with the same lint policy and linter version"
        )
        .into());
    }
    Ok(())
}

impl Capture {
    pub(crate) fn save(&self, exit_code: i32) -> Outcome<i64> {
        fs::create_dir_all(
            self.path
                .parent()
                .ok_or("History database requires a directory")?,
        )?;
        let mut database = connection(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        database.pragma_update(None, "foreign_keys", true)?;
        let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
        schema(&transaction, true)?;
        let finished_at = timestamp()?.max(self.started_at);
        let failure_stage = (exit_code == 2).then(|| self.stage.name());
        transaction.execute(
            "INSERT INTO runs (model_label, started_at, finished_at, exit_code, linter_version,
              policy_fingerprint, baseline_run_id, failure_stage)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                self.model,
                self.started_at,
                finished_at,
                exit_code,
                LINTER_VERSION,
                self.policy,
                self.baseline,
                failure_stage,
            ],
        )?;
        let id = transaction.last_insert_rowid();
        insert_findings(&transaction, id, self)?;
        transaction.commit()?;
        Ok(id)
    }
}

fn insert_findings(database: &Connection, run_id: i64, capture: &Capture) -> Outcome<()> {
    let mut finding =
        database.prepare("INSERT INTO findings (run_id, rule_id, severity) VALUES (?1, ?2, ?3)")?;
    let mut location = database.prepare(
        "INSERT INTO finding_locations (finding_id, ordinal, path, line, column)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for item in &capture.findings {
        finding.execute(params![run_id, item.rule, item.severity.name()])?;
        let finding_id = database.last_insert_rowid();
        for (index, place) in item.locations.iter().enumerate() {
            location.execute(params![
                finding_id,
                i64::try_from(index)?,
                place.path,
                place.line,
                place.column
            ])?;
        }
    }
    Ok(())
}

fn existing(path: &Path) -> Outcome<Connection> {
    if !path.try_exists()? {
        return Err(
            "No lint history recorded for this project. Run check --record --model LABEL.".into(),
        );
    }
    let database = connection(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    schema(&database, false)?;
    Ok(database)
}

fn connection(path: &Path, flags: OpenFlags) -> Outcome<Connection> {
    let database = Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NOFOLLOW)?;
    database.pragma_update(None, "trusted_schema", false)?;
    database.busy_timeout(Duration::from_secs(5))?;
    Ok(database)
}

fn schema(database: &Connection, allow_create: bool) -> Outcome<()> {
    let id = database.pragma_query_value(None, "application_id", |row| row.get::<_, i32>(0))?;
    let version = database.pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0))?;
    if id == APPLICATION_ID && version == SCHEMA_VERSION {
        return Ok(());
    }
    if allow_create && id == 0 && version == 0 {
        let empty = database.query_row(
            "SELECT NOT EXISTS (SELECT 1 FROM sqlite_schema)",
            [],
            |row| row.get::<_, bool>(0),
        )?;
        if empty {
            database.execute_batch(include_str!("database/schema.sql"))?;
            database.pragma_update(None, "application_id", APPLICATION_ID)?;
            database.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            return Ok(());
        }
    }
    Err("Unrecognized or unsupported lint history database schema".into())
}
