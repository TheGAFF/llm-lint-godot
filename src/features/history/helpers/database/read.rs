use super::{existing, history_path};
use crate::{
    features::history::models::{Finding, Location, Report, ReportOptions, Run, Severity},
    shared::models::Outcome,
};
use rusqlite::{Connection, Row, params};
use std::path::Path;

impl Report {
    pub(crate) fn load(root: &Path, options: &ReportOptions) -> Outcome<Self> {
        let database = existing(&history_path(root)?)?;
        let since = options
            .since
            .as_deref()
            .map(|date| since_time(&database, date))
            .transpose()?;
        let mut statement = database.prepare(include_str!("report.sql"))?;
        let mut rows = statement.query(params![options.model, since, options.run])?;
        let mut runs = Vec::new();
        let mut last_finding = None;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get("run_id")?;
            if runs.last().is_none_or(|run: &Run| run.id != id) {
                runs.push(run(row, id)?);
                last_finding = None;
            }
            if let Some(run) = runs.last_mut() {
                append_finding(row, run, &mut last_finding)?;
            }
        }
        Ok(Self::new(runs, options.run.is_some()))
    }
}

fn since_time(database: &Connection, date: &str) -> Outcome<i64> {
    if date.len() != 10 || date.starts_with("0000") {
        return Err("--since requires a valid UTC date in YYYY-MM-DD format".into());
    }
    let timestamp = database.query_row(
        "SELECT CASE WHEN date(?1, '+0 days') = ?1 THEN unixepoch(?1) END",
        [date],
        |row| row.get::<_, Option<i64>>(0),
    )?;
    timestamp.ok_or_else(|| "--since requires a valid UTC date in YYYY-MM-DD format".into())
}

fn run(row: &Row<'_>, id: i64) -> Outcome<Run> {
    Ok(Run {
        id,
        model: row.get("model_label")?,
        finished: row.get("finished")?,
        exit_code: row.get("exit_code")?,
        version: row.get("linter_version")?,
        policy: row.get("policy_fingerprint")?,
        baseline: row.get("baseline_run_id")?,
        failure_stage: row.get("failure_stage")?,
        selected: row.get("selected")?,
        findings: Vec::new(),
    })
}

fn append_finding(row: &Row<'_>, run: &mut Run, last_id: &mut Option<i64>) -> Outcome<()> {
    let Some(id) = row.get::<_, Option<i64>>("finding_id")? else {
        return Ok(());
    };
    if *last_id != Some(id) {
        run.findings.push(Finding {
            rule: row.get("rule_id")?,
            severity: Severity::from_level(row.get_ref("severity")?.as_str()?),
            locations: Vec::new(),
        });
        *last_id = Some(id);
    }
    if let (Some(finding), Some(path)) = (
        run.findings.last_mut(),
        row.get::<_, Option<String>>("path")?,
    ) {
        finding.locations.push(Location {
            path,
            line: row.get("line")?,
            column: row.get("column")?,
        });
    }
    Ok(())
}
