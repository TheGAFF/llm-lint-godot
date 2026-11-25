//! Every endpoint must be explicitly reviewed; an approved function is not a wildcard.
use super::models::{Finding, SuppressedFinding};
use crate::{
    features::configuration::models::{DuplicateException, Project},
    shared::models::{Function, Outcome, Source},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn apply_duplicate_exceptions(
    project: &Project,
    units: &[(&Source, &Function)],
    findings: &mut Vec<Finding>,
) -> Outcome<Vec<SuppressedFinding>> {
    let mut approved = BTreeMap::new();
    for entry in &project.settings.duplicate_exceptions {
        for member in &entry.members {
            let matches: Vec<_> = units
                .iter()
                .filter(|(source, function)| {
                    source.path == member.path && function.name == member.item
                })
                .collect();
            if matches.len() != 1 || matches[0].1.fingerprint != member.fingerprint {
                return Err(format!(
                    "Stale duplicate exception {}: {} [{}] changed, missing or ambiguous",
                    entry.id, member.path, member.item
                )
                .into());
            }
        }
        let mut used_members = BTreeSet::new();
        let mut used_rules = BTreeSet::new();
        for (index, finding) in findings.iter().enumerate() {
            if finding.severity != "warning"
                || !entry.rules.contains(&finding.rule)
                || !reviewed(entry, finding)
            {
                continue;
            }
            if approved.insert(index, entry).is_some() {
                return Err("Overlapping duplicate exceptions".into());
            }
            used_rules.insert(&finding.rule);
            for occurrence in &finding.occurrences {
                used_members.insert((&occurrence.path, &occurrence.item));
            }
        }
        if used_members.len() != entry.members.len() || used_rules.len() != entry.rules.len() {
            return Err(format!(
                "Stale duplicate exception {}: remove unused members or rules",
                entry.id
            )
            .into());
        }
    }
    let mut suppressed = Vec::new();
    let mut active = Vec::new();
    for (index, finding) in findings.drain(..).enumerate() {
        if let Some(entry) = approved.get(&index) {
            suppressed.push(SuppressedFinding {
                exception_id: entry.id.clone(),
                reason: entry.reason.clone(),
                finding,
            });
        } else {
            active.push(finding);
        }
    }
    *findings = active;
    Ok(suppressed)
}

fn reviewed(entry: &DuplicateException, finding: &Finding) -> bool {
    finding.occurrences.iter().all(|o| {
        entry
            .members
            .iter()
            .any(|m| m.path == o.path && m.item == o.item && m.fingerprint == o.fingerprint)
    })
}
