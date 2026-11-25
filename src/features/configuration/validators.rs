use super::{helpers::safe_path, models::Settings};
use crate::shared::models::Outcome;
use std::collections::BTreeSet;

pub(super) fn validate_duplicate_exceptions(settings: &Settings) -> Outcome<()> {
    let mut ids = BTreeSet::new();
    for entry in &settings.duplicate_exceptions {
        if !settings.duplicates.enabled
            || entry.id.trim().is_empty()
            || entry.reason.trim().is_empty()
            || !ids.insert(&entry.id)
            || entry.members.len() < 2
            || entry.rules.is_empty()
        {
            return Err("Invalid or repeated duplicate exception".into());
        }
        let mut rules = BTreeSet::new();
        for rule in &entry.rules {
            if ![
                "duplicates/exact",
                "duplicates/renamed",
                "duplicates/similar",
            ]
            .contains(&rule.as_str())
                || !rules.insert(rule)
            {
                return Err(
                    "Duplicate exceptions require distinct exact duplicate rule IDs".into(),
                );
            }
        }
        let mut members = BTreeSet::new();
        for member in &entry.members {
            safe_path(&member.path)?;
            if !member.path.ends_with(".gd")
                || member.item.trim().is_empty()
                || member.item.contains(['*', '?'])
                || member.fingerprint.len() != 64
                || !member
                    .fingerprint
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                || !members.insert((&member.path, &member.item))
            {
                return Err("Invalid or repeated duplicate exception member".into());
            }
        }
    }
    Ok(())
}
