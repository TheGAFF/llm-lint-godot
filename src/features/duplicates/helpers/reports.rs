use super::{
    super::mutators::apply_duplicate_exceptions,
    matches::{candidates, code_lines},
};
use crate::{
    features::{
        architecture::helpers::corpus,
        configuration::{
            helpers::{inside, target_paths},
            models::{DuplicateSettings, Project},
        },
        duplicates::models::{
            Category, DuplicateReport, Finding, Guidance, Occurrence, Position, Summary,
            SuppressedFinding, WarningCounts,
        },
    },
    shared::models::{Corpus, Function, Outcome, Source, Token},
};
use std::collections::{BTreeSet, HashMap};

pub fn analyze_duplicates(project: &Project, targets: &[String]) -> Outcome<DuplicateReport> {
    if !project.settings.duplicates.enabled {
        return Err("Duplicate detection is disabled".into());
    }
    let corpus = corpus(project)?;
    analyze_duplicate_corpus(project, &corpus, targets)
}

pub(crate) fn analyze_duplicate_corpus(
    project: &Project,
    corpus: &Corpus,
    targets: &[String],
) -> Outcome<DuplicateReport> {
    if corpus
        .sources
        .iter()
        .any(|source| !source.diagnostics.is_empty())
    {
        return Err("Cannot analyze duplicates in malformed or unsupported GDScript".into());
    }
    let targets = target_paths(project, targets)?;
    let units: Vec<_> = corpus
        .sources
        .iter()
        .flat_map(|source| {
            source
                .functions
                .iter()
                .map(move |function| (source, function))
        })
        .collect();
    let settings = &project.settings.duplicates;
    let mut findings = Vec::new();
    for candidate in candidates(
        &units,
        settings,
        &targets,
        &project.settings.duplicate_exceptions,
    ) {
        let (left, right) = (units[candidate.left], units[candidate.right]);
        let (left_tokens, right_tokens) = if candidate.kind == "similar" {
            (left.1.tokens.as_slice(), right.1.tokens.as_slice())
        } else {
            (
                &left.1.tokens[candidate.left_start..candidate.left_start + candidate.length],
                &right.1.tokens[candidate.right_start..candidate.right_start + candidate.length],
            )
        };
        let occurrences = vec![
            occurrence(left, left_tokens)?,
            occurrence(right, right_tokens)?,
        ];
        let error = candidate.kind == "exact"
            && occurrences.iter().all(|occurrence| {
                occurrence.tokens >= settings.error_tokens
                    && occurrence.code_lines >= settings.error_lines
            });
        findings.push(Finding {
            category: occurrences[0].category.min(occurrences[1].category),
            rule: format!("duplicates/{}", candidate.kind),
            kind: candidate.kind.into(),
            severity: if error { "error" } else { "warning" }.into(),
            similarity: candidate.similarity,
            occurrences,
            guidance: guidance(left, right),
        });
    }
    findings.sort_by(|a, b| {
        (
            &a.occurrences[0].path,
            a.occurrences[0].start.offset,
            &a.occurrences[1].path,
            a.occurrences[1].start.offset,
            &a.kind,
        )
            .cmp(&(
                &b.occurrences[0].path,
                b.occurrences[0].start.offset,
                &b.occurrences[1].path,
                b.occurrences[1].start.offset,
                &b.kind,
            ))
    });
    apply_levels(project, &mut findings);
    findings = redundant(findings);
    // Validate exceptions against the full corpus, even for targeted invocations.
    let mut suppressed = apply_duplicate_exceptions(project, &units, &mut findings)?;
    findings.retain(|finding| selected(finding, &targets));
    suppressed.retain(|entry| selected(&entry.finding, &targets));
    Ok(report(corpus, settings, units.len(), findings, suppressed))
}

fn report(
    corpus: &Corpus,
    settings: &DuplicateSettings,
    units: usize,
    findings: Vec<Finding>,
    suppressed: Vec<SuppressedFinding>,
) -> DuplicateReport {
    let errors = findings
        .iter()
        .filter(|finding| finding.severity == "error")
        .count();
    let warnings = findings.len() - errors;
    let mut warnings_by_category = WarningCounts::default();
    for finding in findings
        .iter()
        .filter(|finding| finding.severity == "warning")
    {
        warnings_by_category.add(finding.category);
    }
    DuplicateReport {
        version: 1,
        engine: "gdscript-tree-sitter-6.1.0",
        thresholds: settings.clone(),
        summary: Summary {
            scanned_files: corpus
                .sources
                .iter()
                .filter(|source| source.path.ends_with(".gd"))
                .count(),
            analyzed_units: units,
            errors,
            warnings,
            warnings_by_category,
            suppressed: suppressed.len(),
        },
        diagnostics: findings,
        suppressed,
    }
}

fn occurrence(unit: (&Source, &Function), tokens: &[Token]) -> Outcome<Occurrence> {
    let first = tokens.first().ok_or("Empty duplicate occurrence")?;
    let last = tokens.last().ok_or("Empty duplicate occurrence")?;
    let before = &unit.0.text[..last.end];
    Ok(Occurrence {
        category: Category::from_path(&unit.0.path),
        fingerprint: unit.1.fingerprint.clone(),
        path: unit.0.path.clone(),
        item: unit.1.name.clone(),
        start: Position {
            line: first.line,
            column: first.column,
            offset: first.start,
        },
        end: Position {
            line: last.end_line,
            column: before.rsplit('\n').next().unwrap_or("").chars().count() + 1,
            offset: last.end,
        },
        tokens: tokens.len(),
        code_lines: code_lines(tokens),
    })
}

fn selected(finding: &Finding, targets: &[String]) -> bool {
    targets.is_empty()
        || finding
            .occurrences
            .iter()
            .any(|o| targets.iter().any(|target| inside(&o.path, target)))
}

fn guidance(left: (&Source, &Function), right: (&Source, &Function)) -> Guidance {
    let blockers: BTreeSet<_> = left
        .1
        .blockers
        .iter()
        .chain(&right.1.blockers)
        .cloned()
        .collect();
    let (kind, destination, message) = if !blockers.is_empty() {
        (
            "review",
            None,
            "Inspect state, captures and runtime dependencies before extracting",
        )
    } else if left.0.path == right.0.path {
        (
            "private",
            Some(left.0.path.clone()),
            "Prefer a private helper in this script",
        )
    } else if left.0.owner == right.0.owner {
        (
            "owner",
            Some(left.0.owner.clone()),
            "Keep reusable domain logic within this owner",
        )
    } else {
        (
            "shared",
            None,
            "Use shared only for neutral logic; otherwise retain the domain owner",
        )
    };
    Guidance {
        kind: kind.into(),
        destination,
        message: message.into(),
        blockers: blockers.into_iter().collect(),
    }
}

fn redundant(findings: Vec<Finding>) -> Vec<Finding> {
    let mut pairs = HashMap::<(&str, &str), Vec<usize>>::new();
    for (index, finding) in findings.iter().enumerate() {
        pairs.entry(file_pair(finding)).or_default().push(index);
    }
    let keep = findings
        .iter()
        .enumerate()
        .map(|(index, finding)| {
            !pairs[&file_pair(finding)]
                .iter()
                .any(|&other| index != other && subsumes(&findings[other], finding, other < index))
        })
        .collect::<Vec<_>>();
    findings
        .into_iter()
        .zip(keep)
        .filter_map(|(finding, keep)| keep.then_some(finding))
        .collect()
}

fn file_pair(finding: &Finding) -> (&str, &str) {
    (&finding.occurrences[0].path, &finding.occurrences[1].path)
}

fn subsumes(outer: &Finding, inner: &Finding, earlier: bool) -> bool {
    if inner.severity == "error" && outer.severity != "error" {
        return false;
    }
    let mut strict = false;
    for (a, b) in outer.occurrences.iter().zip(&inner.occurrences) {
        if a.start.offset > b.start.offset || a.end.offset < b.end.offset {
            return false;
        }
        strict |= a.start.offset < b.start.offset || a.end.offset > b.end.offset;
    }
    strict
        || (outer.kind == "exact" && inner.kind != "exact")
        || (outer.kind == inner.kind && earlier)
}

fn apply_levels(project: &Project, findings: &mut Vec<Finding>) {
    findings.retain_mut(|finding| {
        let default = if finding.severity == "error" {
            "error"
        } else {
            "warn"
        };
        finding.occurrences.retain(|item| {
            crate::features::configuration::rules::level(
                &project.settings.rules,
                &project.settings.overrides,
                &finding.rule,
                &item.path,
                default,
            ) != "off"
        });
        let error = finding.occurrences.iter().any(|item| {
            crate::features::configuration::rules::level(
                &project.settings.rules,
                &project.settings.overrides,
                &finding.rule,
                &item.path,
                default,
            ) == "error"
        });
        finding.severity = if error { "error" } else { "warning" }.into();
        finding.occurrences.len() > 1
    });
}
