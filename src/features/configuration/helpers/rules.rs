use crate::shared::models::Outcome;
use std::collections::BTreeMap;

pub use super::super::models::{RuleOverride, RuleSetting};

pub fn matches(pattern: &str, path: &str) -> bool {
    let pattern: Vec<_> = pattern.split('/').collect();
    let path: Vec<_> = path.split('/').collect();
    let mut state = vec![vec![false; path.len() + 1]; pattern.len() + 1];
    state[0][0] = true;
    for (i, part) in pattern.iter().enumerate() {
        for j in 0..=path.len() {
            if *part == "**" {
                state[i + 1][j] = state[i][j] || (j > 0 && state[i + 1][j - 1]);
            } else if j > 0 {
                state[i + 1][j] = state[i][j - 1] && segment_matches(part, path[j - 1]);
            }
        }
    }
    state[pattern.len()][path.len()]
}

fn segment_matches(pattern: &str, path: &str) -> bool {
    let pattern: Vec<_> = pattern.chars().collect();
    let path: Vec<_> = path.chars().collect();
    let mut state = vec![vec![false; path.len() + 1]; pattern.len() + 1];
    state[0][0] = true;
    for (i, part) in pattern.iter().enumerate() {
        for j in 0..=path.len() {
            state[i + 1][j] = match part {
                '*' => state[i][j] || (j > 0 && state[i + 1][j - 1]),
                '?' => j > 0 && state[i][j - 1],
                _ => j > 0 && state[i][j - 1] && *part == path[j - 1],
            };
        }
    }
    state[pattern.len()][path.len()]
}

pub fn level<'a>(
    rules: &'a BTreeMap<String, RuleSetting>,
    overrides: &'a [RuleOverride],
    rule: &str,
    path: &str,
    default: &'a str,
) -> &'a str {
    let mut result = rules
        .get(rule)
        .and_then(RuleSetting::level)
        .unwrap_or(default);
    for entry in overrides {
        if !entry.includes.iter().any(|pattern| matches(pattern, path)) {
            continue;
        }
        if let Some(value) = entry.rules.get(rule).and_then(RuleSetting::level) {
            result = value;
        }
    }
    result
}

pub fn catalog() -> Outcome<Vec<serde_json::Value>> {
    Ok(serde_json::from_str(include_str!(
        "../../../../policy/rules.json"
    ))?)
}

pub fn rules_validity(
    rules: &BTreeMap<String, RuleSetting>,
    overrides: &[RuleOverride],
) -> Outcome<()> {
    let catalog = catalog()?;
    validate_map(rules, &catalog, false)?;
    for entry in overrides {
        if entry.includes.is_empty()
            || entry.includes.iter().any(|path| {
                path.is_empty()
                    || path.starts_with('/')
                    || path.contains(['\\', ':', '[', ']', '{', '}'])
                    || path
                        .split('/')
                        .any(|part| part == ".." || part == "." || part.is_empty())
            })
        {
            return Err(
                "Rule overrides require project-relative glob patterns using *, ** or ?".into(),
            );
        }
        validate_map(&entry.rules, &catalog, true)?;
    }
    Ok(())
}

fn validate_map(
    rules: &BTreeMap<String, RuleSetting>,
    catalog: &[serde_json::Value],
    scoped: bool,
) -> Outcome<()> {
    for (id, setting) in rules {
        let definition = catalog
            .iter()
            .find(|entry| entry["id"].as_str() == Some(id))
            .ok_or_else(|| format!("Unknown rule: {id}; run the rules command"))?;
        if let Some(level) = setting.level() {
            if !["off", "warn", "error"].contains(&level) {
                return Err(format!("Invalid level for {id}: expected off, warn or error").into());
            }
            if definition["configurable"] == false && level != "error" {
                return Err(
                    format!("{id} is an input-integrity check and must remain error").into(),
                );
            }
        }
        validate_options(id, setting, definition, scoped)?;
    }
    Ok(())
}

pub fn threshold(
    rules: &BTreeMap<String, RuleSetting>,
    rule: &str,
    key: &str,
    default: usize,
) -> usize {
    match rules.get(rule) {
        Some(RuleSetting::Detailed(value)) => value
            .options
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .and_then(|number| usize::try_from(number).ok())
            .unwrap_or(default),
        _ => default,
    }
}

fn validate_options(
    id: &str,
    setting: &RuleSetting,
    definition: &serde_json::Value,
    scoped: bool,
) -> Outcome<()> {
    let RuleSetting::Detailed(value) = setting else {
        return Ok(());
    };
    if value.level.is_none() && value.options.is_empty() {
        return Err(format!("Empty rule setting: {id}").into());
    }
    if super::contracts::validity(id, &value.options)? {
        return Ok(());
    }
    for (key, value) in &value.options {
        if (scoped && !id.starts_with("architecture/"))
            || !definition["options"]
                .as_array()
                .is_some_and(|keys| keys.iter().any(|item| item == key))
            || !value.as_u64().is_some_and(|number| number > 0)
        {
            return Err(format!("Unsupported option {key} for {id}; native thresholds are project-wide positive integers").into());
        }
    }
    Ok(())
}

pub fn threshold_for_path(
    rules: &BTreeMap<String, RuleSetting>,
    overrides: &[RuleOverride],
    rule: &str,
    key: &str,
    path: &str,
    default: usize,
) -> usize {
    let mut result = threshold(rules, rule, key, default);
    for entry in overrides {
        if entry.includes.iter().any(|pattern| matches(pattern, path)) {
            result = threshold(&entry.rules, rule, key, result);
        }
    }
    result
}
