use crate::{
    features::{configuration::models::DuplicateSettings, duplicates::models::Match},
    shared::models::{Function, Source, Token},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    hash::{Hash, Hasher},
};

pub fn candidates(units: &[(&Source, &Function)], settings: &DuplicateSettings) -> Vec<Match> {
    let mut output = fragments(units, settings, false);
    output.extend(fragments(units, settings, true));
    for left in 0..units.len() {
        for right in left + 1..units.len() {
            let (a, b) = (units[left].1, units[right].1);
            if !eligible(&a.tokens, settings)
                || !eligible(&b.tokens, settings)
                || overlapping(units[left], units[right])
            {
                continue;
            }
            if output
                .iter()
                .any(|found| found.left == left && found.right == right)
            {
                continue;
            }
            let similarity = similarity(&a.shape, &b.shape);
            if similarity >= settings.similarity {
                output.push(Match {
                    left,
                    right,
                    left_start: 0,
                    right_start: 0,
                    length: 0,
                    kind: "similar",
                    similarity: Some(similarity),
                });
            }
        }
    }
    output
}

fn fragments(
    units: &[(&Source, &Function)],
    settings: &DuplicateSettings,
    renamed: bool,
) -> Vec<Match> {
    let size = settings.warning_tokens;
    let mut windows: BTreeMap<u64, Vec<(usize, usize)>> = BTreeMap::new();
    let mut output = Vec::new();
    let mut seen = BTreeSet::new();
    for (unit, (_, function)) in units.iter().enumerate() {
        for start in 0..function.tokens.len().saturating_sub(size - 1) {
            let tokens = &function.tokens[start..start + size];
            let signature = signature(tokens, renamed);
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            signature.hash(&mut hash);
            let previous = windows.entry(hash.finish()).or_default();
            let found = window_matches(units, previous, (unit, start), settings, renamed, &output);
            output.extend(found.into_iter().filter(|found| {
                seen.insert((
                    found.left,
                    found.right,
                    found.left_start,
                    found.right_start,
                    found.length,
                ))
            }));
            previous.push((unit, start));
        }
    }
    output
}

fn window_matches(
    units: &[(&Source, &Function)],
    previous: &[(usize, usize)],
    current: (usize, usize),
    settings: &DuplicateSettings,
    renamed: bool,
    output: &[Match],
) -> Vec<Match> {
    let (unit, start) = current;
    let size = settings.warning_tokens;
    let mut result = Vec::new();
    for &(other, offset) in previous {
        let covered = output.iter().any(|found| {
            found.left == other
                && found.right == unit
                && offset >= found.left_start
                && offset + size <= found.left_start + found.length
                && start >= found.right_start
                && offset - found.left_start == start - found.right_start
        });
        if covered {
            continue;
        }
        if let Some(found) = expanded(units, (other, unit, offset, start), settings, renamed) {
            result.push(found);
        }
    }
    result
}

fn expanded(
    units: &[(&Source, &Function)],
    pair: (usize, usize, usize, usize),
    settings: &DuplicateSettings,
    renamed: bool,
) -> Option<Match> {
    let (other, unit, mut left, mut right) = pair;
    if overlapping(units[unit], units[other]) {
        return None;
    }
    let other_tokens = &units[other].1.tokens;
    let tokens = &units[unit].1.tokens;
    let mut length = settings.warning_tokens;
    if !equivalent(
        &other_tokens[left..left + length],
        &tokens[right..right + length],
        renamed,
    ) {
        return None;
    }
    while left > 0
        && right > 0
        && equivalent(
            &other_tokens[left - 1..left + length],
            &tokens[right - 1..right + length],
            renamed,
        )
    {
        left -= 1;
        right -= 1;
        length += 1;
    }
    while left + length < other_tokens.len()
        && right + length < tokens.len()
        && equivalent(
            &other_tokens[left..left + length + 1],
            &tokens[right..right + length + 1],
            renamed,
        )
    {
        length += 1;
    }
    let (a, b) = (
        &other_tokens[left..left + length],
        &tokens[right..right + length],
    );
    if !eligible(a, settings) || !eligible(b, settings) || (renamed && equivalent(a, b, false)) {
        return None;
    }
    Some(Match {
        left: other,
        right: unit,
        left_start: left,
        right_start: right,
        length,
        kind: if renamed { "renamed" } else { "exact" },
        similarity: None,
    })
}

fn signature(tokens: &[Token], renamed: bool) -> Vec<String> {
    let mut names: BTreeMap<&str, usize> = BTreeMap::new();
    tokens
        .iter()
        .map(|token| {
            if renamed && ["identifier", "name"].contains(&token.kind.as_str()) {
                let next = names.len();
                format!(
                    "{}#{}",
                    token.kind,
                    names.entry(&token.text).or_insert(next)
                )
            } else {
                format!("{}:{}", token.kind, token.text)
            }
        })
        .collect()
}

fn equivalent(left: &[Token], right: &[Token], renamed: bool) -> bool {
    signature(left, renamed) == signature(right, renamed)
}

pub fn code_lines(tokens: &[Token]) -> usize {
    tokens
        .iter()
        .flat_map(|token| token.line..=token.end_line)
        .collect::<BTreeSet<_>>()
        .len()
}

fn eligible(tokens: &[Token], settings: &DuplicateSettings) -> bool {
    tokens.len() >= settings.warning_tokens && code_lines(tokens) >= settings.warning_lines
}

fn overlapping(left: (&Source, &Function), right: (&Source, &Function)) -> bool {
    if left.0.path != right.0.path {
        return false;
    }
    match (
        left.1.tokens.first(),
        left.1.tokens.last(),
        right.1.tokens.first(),
        right.1.tokens.last(),
    ) {
        (Some(a), Some(b), Some(c), Some(d)) => a.start < d.end && c.start < b.end,
        _ => false,
    }
}

fn similarity(left: &[String], right: &[String]) -> f64 {
    let counts = |shape: &[String]| {
        let mut counts = BTreeMap::new();
        for pair in shape.windows(2) {
            *counts
                .entry((pair[0].clone(), pair[1].clone()))
                .or_insert(0usize) += 1;
        }
        counts
    };
    let (left, right) = (counts(left), counts(right));
    let total: usize = left.values().sum::<usize>() + right.values().sum::<usize>();
    if total == 0 {
        return 0.0;
    }
    let common: usize = left
        .iter()
        .map(|(key, count)| (*count).min(*right.get(key).unwrap_or(&0)))
        .sum();
    2.0 * common as f64 / total as f64
}
