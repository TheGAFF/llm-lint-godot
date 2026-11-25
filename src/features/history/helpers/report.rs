use super::super::models::{Group, Location, Report, Run, Severity, Table};
use std::{cmp::Reverse, collections::BTreeMap};

type RuleCounts = BTreeMap<(String, Severity), i64>;

#[derive(Default)]
struct ModelTotals {
    runs: i64,
    complete: i64,
    passed: i64,
    errors: i64,
    warnings: i64,
    paired: i64,
}

#[derive(Default)]
struct RuleTotals {
    runs: i64,
    findings: i64,
}

#[derive(Default)]
struct Change {
    fewer: i64,
    more: i64,
}

impl Report {
    pub(super) fn new(runs: Vec<Run>, detail: bool) -> Self {
        let counts: BTreeMap<_, _> = runs
            .iter()
            .filter(|run| run.exit_code != 2)
            .map(|run| (run.id, rule_counts(run)))
            .collect();
        let mut groups: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for run in runs.into_iter().filter(|run| run.selected) {
            groups
                .entry((run.version.clone(), run.policy.clone()))
                .or_default()
                .push(run);
        }
        Self {
            groups: groups
                .into_iter()
                .map(|((version, policy), runs)| group(version, policy, runs, &counts))
                .collect(),
            detail,
        }
    }
}

fn group(
    version: String,
    policy: String,
    mut runs: Vec<Run>,
    counts: &BTreeMap<i64, RuleCounts>,
) -> Group {
    let mut models: BTreeMap<_, ModelTotals> = BTreeMap::new();
    let mut rules: BTreeMap<_, RuleTotals> = BTreeMap::new();
    let mut changes: BTreeMap<_, Change> = BTreeMap::new();
    for run in &runs {
        let model = models.entry(run.model.as_str()).or_default();
        model.runs += 1;
        let Some(current) = counts.get(&run.id) else {
            continue;
        };
        model.complete += 1;
        model.passed += i64::from(run.exit_code == 0);
        let (errors, warnings) = totals(current);
        model.errors += errors;
        model.warnings += warnings;
        for (rule, count) in current {
            let total = rules.entry(rule).or_default();
            total.runs += 1;
            total.findings += count;
        }
        if let Some(baseline) = run.baseline.and_then(|id| counts.get(&id)) {
            model.paired += 1;
            compare(&run.model, current, baseline, &mut changes);
        }
    }
    let tables = vec![
        models_table(models),
        rules_table(rules),
        changes_table(changes),
    ];
    runs.sort_by(|left, right| (&right.finished, right.id).cmp(&(&left.finished, left.id)));
    let mut tables = tables;
    tables.push(timeline(&runs, counts));
    Group {
        version,
        policy,
        tables,
        runs,
    }
}

fn compare<'a>(
    model: &'a str,
    current: &'a RuleCounts,
    baseline: &'a RuleCounts,
    changes: &mut BTreeMap<(&'a str, &'a (String, Severity)), Change>,
) {
    for (key, count) in current {
        let delta = count - baseline.get(key).copied().unwrap_or(0);
        if delta > 0 {
            changes.entry((model, key)).or_default().more += delta;
        }
    }
    for (key, count) in baseline {
        let delta = count - current.get(key).copied().unwrap_or(0);
        if delta > 0 {
            changes.entry((model, key)).or_default().fewer += delta;
        }
    }
}

fn rule_counts(run: &Run) -> RuleCounts {
    let mut counts = BTreeMap::new();
    for finding in &run.findings {
        *counts
            .entry((finding.rule.clone(), finding.severity))
            .or_default() += 1;
    }
    counts
}

fn totals(counts: &RuleCounts) -> (i64, i64) {
    let mut totals = (0, 0);
    for ((_, severity), count) in counts {
        match severity {
            Severity::Error => totals.0 += count,
            Severity::Warning => totals.1 += count,
        }
    }
    totals
}

fn average(value: i64, count: i64, percentage: bool) -> String {
    if count == 0 {
        return "—".into();
    }
    let value = value as f64 / count as f64;
    if percentage {
        format!("{:.1}%", value * 100.0)
    } else {
        format!("{value:.1}")
    }
}

fn models_table(models: BTreeMap<&str, ModelTotals>) -> Table {
    Table {
        title: "Models",
        columns: &[
            "Model",
            "Runs",
            "Complete",
            "Pass rate",
            "Errors/run",
            "Warnings/run",
            "Paired runs",
        ],
        rows: models
            .into_iter()
            .map(|(name, model)| {
                vec![
                    name.into(),
                    model.runs.to_string(),
                    model.complete.to_string(),
                    average(model.passed, model.complete, true),
                    average(model.errors, model.complete, false),
                    average(model.warnings, model.complete, false),
                    model.paired.to_string(),
                ]
            })
            .collect(),
    }
}

fn rules_table(rules: BTreeMap<&(String, Severity), RuleTotals>) -> Table {
    let mut rules: Vec<_> = rules.into_iter().collect();
    rules.sort_by(|(left, a), (right, b)| {
        (Reverse(a.runs), Reverse(a.findings), left).cmp(&(
            Reverse(b.runs),
            Reverse(b.findings),
            right,
        ))
    });
    Table {
        title: "Most frequent rules",
        columns: &["Rule", "Severity", "Affected runs", "Findings"],
        rows: rules
            .into_iter()
            .map(|((rule, severity), totals)| {
                vec![
                    rule.clone(),
                    severity.name().into(),
                    totals.runs.to_string(),
                    totals.findings.to_string(),
                ]
            })
            .collect(),
    }
}

fn changes_table(changes: BTreeMap<(&str, &(String, Severity)), Change>) -> Table {
    Table {
        title: "Observed changes from selected baselines",
        columns: &[
            "Follow-up model",
            "Rule",
            "Severity",
            "Fewer findings",
            "More findings",
        ],
        rows: changes
            .into_iter()
            .map(|((model, (rule, severity)), change)| {
                vec![
                    model.into(),
                    rule.clone(),
                    severity.name().into(),
                    change.fewer.to_string(),
                    change.more.to_string(),
                ]
            })
            .collect(),
    }
}

fn timeline(runs: &[Run], counts: &BTreeMap<i64, RuleCounts>) -> Table {
    Table {
        title: "Run timeline (UTC)",
        columns: &[
            "Run", "Finished", "Model", "Result", "Errors", "Warnings", "Baseline",
        ],
        rows: runs
            .iter()
            .map(|run| {
                let (errors, warnings) = counts.get(&run.id).map_or_else(
                    || ("—".into(), "—".into()),
                    |rules| {
                        let (errors, warnings) = totals(rules);
                        (errors.to_string(), warnings.to_string())
                    },
                );
                vec![
                    run.id.to_string(),
                    run.finished.clone(),
                    run.model.clone(),
                    run.status(),
                    errors,
                    warnings,
                    run.baseline.map_or_else(|| "—".into(), |id| id.to_string()),
                ]
            })
            .collect(),
    }
}

impl Run {
    pub(super) fn status(&self) -> String {
        match &self.failure_stage {
            Some(stage) => format!("Incomplete ({stage})"),
            None if self.exit_code == 0 => "Passed".into(),
            None => "Failed lint".into(),
        }
    }

    pub(super) fn detail(&self) -> Table {
        Table {
            title: "Findings recorded before this run ended",
            columns: &["Rule", "Severity", "Locations"],
            rows: self
                .findings
                .iter()
                .map(|finding| {
                    let locations: Vec<_> =
                        finding.locations.iter().map(Location::display).collect();
                    vec![
                        finding.rule.clone(),
                        finding.severity.name().into(),
                        locations.join(" ↔ "),
                    ]
                })
                .collect(),
        }
    }
}
