use crate::{
    features::{
        configuration::models::{DuplicateException, DuplicateSettings},
        duplicates::models::Match,
    },
    shared::models::{Function, Source, Token},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    hash::{Hash, Hasher},
};

mod scope;
mod semantic;
use scope::Scope;

pub(super) fn candidates(
    units: &[(&Source, &Function)],
    settings: &DuplicateSettings,
    targets: &[String],
    exceptions: &[DuplicateException],
) -> Vec<Match> {
    let scope = Scope::new(units, targets, exceptions);
    let mut output = fragments(units, settings, false, scope.as_ref());
    output.extend(fragments(units, settings, true, scope.as_ref()));
    append_similar(units, settings, scope.as_ref(), &mut output);
    output
}

fn append_similar(
    units: &[(&Source, &Function)],
    settings: &DuplicateSettings,
    scope: Option<&Scope>,
    output: &mut Vec<Match>,
) {
    let pairs: BTreeSet<_> = output
        .iter()
        .map(|found| (found.left, found.right))
        .collect();
    let eligible: Vec<_> = units
        .iter()
        .enumerate()
        .filter_map(|(index, (_, f))| eligible(&f.tokens, settings).then_some(index))
        .collect();
    let mut symbols = HashMap::new();
    let shapes: Vec<_> = units
        .iter()
        .map(|(_, f)| Shape::new(&f.shape, &mut symbols))
        .collect();
    let partners = scope.map(|scope| scope.partners(&eligible));
    let mut merged = Vec::new();
    for (index, &left) in eligible.iter().enumerate() {
        let all_after = &eligible[index + 1..];
        let partners = match &partners {
            Some(partners) => partners.after(left, all_after, &mut merged),
            None => all_after,
        };
        for &right in partners {
            if overlapping(units[left], units[right]) {
                continue;
            }
            if pairs.contains(&(left, right)) {
                continue;
            }
            let total = shapes[left].total + shapes[right].total;
            if total > 0
                && 2.0 * shapes[left].total.min(shapes[right].total) as f64 / (total as f64)
                    < settings.similarity
            {
                continue;
            }
            let similarity = similarity(&shapes[left], &shapes[right]);
            if similarity >= settings.similarity
                && semantic::compatible(units[left].1, units[right].1, settings.similarity)
            {
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
}

fn fragments(
    units: &[(&Source, &Function)],
    settings: &DuplicateSettings,
    renamed: bool,
    scope: Option<&Scope>,
) -> Vec<Match> {
    let size = settings.warning_tokens;
    let mut windows: HashMap<u64, Vec<(usize, usize)>> = HashMap::new();
    let mut bindings = HashMap::new();
    let mut output = Vec::new();
    let mut seen = BTreeSet::new();
    let mut coverage: BTreeMap<(usize, usize), Vec<Match>> = BTreeMap::new();
    for (unit, (_, function)) in units.iter().enumerate() {
        for start in 0..function.tokens.len().saturating_sub(size - 1) {
            let tokens = &function.tokens[start..start + size];
            let previous = windows
                .entry(window_hash(tokens, renamed, &mut bindings))
                .or_default();
            let eligible = previous
                .iter()
                .copied()
                .filter(|(other, _)| scope.is_none_or(|scope| scope.includes(*other, unit)));
            let found =
                window_matches(units, eligible, (unit, start), settings, renamed, &coverage);
            for found in found.into_iter().filter(|found| {
                seen.insert((
                    found.left,
                    found.right,
                    found.left_start,
                    found.right_start,
                    found.length,
                ))
            }) {
                coverage
                    .entry((found.left, found.right))
                    .or_default()
                    .push(found.clone());
                output.push(found);
            }
            previous.push((unit, start));
        }
    }
    output
}

fn window_matches(
    units: &[(&Source, &Function)],
    previous: impl Iterator<Item = (usize, usize)>,
    current: (usize, usize),
    settings: &DuplicateSettings,
    renamed: bool,
    coverage: &BTreeMap<(usize, usize), Vec<Match>>,
) -> Vec<Match> {
    let (unit, start) = current;
    let size = settings.warning_tokens;
    let mut result = Vec::new();
    for (other, offset) in previous {
        let covered = coverage
            .get(&(other, unit))
            .into_iter()
            .flatten()
            .any(|found| {
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
    let mut comparison = Equivalence::new(renamed);
    if !other_tokens[left..left + length]
        .iter()
        .zip(&tokens[right..right + length])
        .all(|(a, b)| comparison.accept(a, b))
    {
        return None;
    }
    while left > 0 && right > 0 && comparison.accept(&other_tokens[left - 1], &tokens[right - 1]) {
        left -= 1;
        right -= 1;
        length += 1;
    }
    while left + length < other_tokens.len()
        && right + length < tokens.len()
        && comparison.accept(&other_tokens[left + length], &tokens[right + length])
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

fn window_hash(tokens: &[Token], renamed: bool, names: &mut HashMap<usize, usize>) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    names.clear();
    for token in tokens {
        token.kind.hash(&mut hash);
        if let Some(binding) = token.binding.filter(|_| renamed) {
            0_u8.hash(&mut hash);
            let next = names.len();
            names.entry(binding).or_insert(next).hash(&mut hash);
        } else {
            1_u8.hash(&mut hash);
            token.text.hash(&mut hash);
        }
    }
    hash.finish()
}

struct Equivalence {
    renamed: bool,
    forward: HashMap<usize, usize>,
    backward: HashMap<usize, usize>,
}

impl Equivalence {
    fn new(renamed: bool) -> Self {
        Self {
            renamed,
            forward: HashMap::new(),
            backward: HashMap::new(),
        }
    }

    fn accept(&mut self, left: &Token, right: &Token) -> bool {
        if left.kind != right.kind {
            return false;
        }
        if !self.renamed {
            return left.text == right.text;
        }
        let (Some(a), Some(b)) = (left.binding, right.binding) else {
            return left.binding == right.binding && left.text == right.text;
        };
        if self.forward.get(&a).is_some_and(|value| *value != b)
            || self.backward.get(&b).is_some_and(|value| *value != a)
        {
            return false;
        }
        self.forward.insert(a, b);
        self.backward.insert(b, a);
        true
    }
}

fn equivalent(left: &[Token], right: &[Token], renamed: bool) -> bool {
    let mut comparison = Equivalence::new(renamed);
    left.len() == right.len() && left.iter().zip(right).all(|(a, b)| comparison.accept(a, b))
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

struct Shape {
    counts: Vec<(usize, usize)>,
    total: usize,
}

impl Shape {
    fn new(shape: &[String], symbols: &mut HashMap<(String, String), usize>) -> Self {
        let mut counts = BTreeMap::new();
        for pair in shape.windows(2) {
            let next = symbols.len();
            let symbol = *symbols
                .entry((pair[0].clone(), pair[1].clone()))
                .or_insert(next);
            *counts.entry(symbol).or_insert(0) += 1;
        }
        Self {
            counts: counts.into_iter().collect(),
            total: shape.len().saturating_sub(1),
        }
    }
}

fn similarity(left: &Shape, right: &Shape) -> f64 {
    let total = left.total + right.total;
    if total == 0 {
        return 0.0;
    }
    let (mut a, mut b, mut common) = (0, 0, 0);
    while a < left.counts.len() && b < right.counts.len() {
        match left.counts[a].0.cmp(&right.counts[b].0) {
            std::cmp::Ordering::Less => a += 1,
            std::cmp::Ordering::Greater => b += 1,
            std::cmp::Ordering::Equal => {
                common += left.counts[a].1.min(right.counts[b].1);
                a += 1;
                b += 1;
            }
        }
    }
    2.0 * common as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use super::{Shape, similarity};
    use std::collections::HashMap;
    fn reference_similarity(left: &[String], right: &[String]) -> f64 {
        let mut remaining: Vec<_> = right.windows(2).collect();
        let mut common = 0;
        for pair in left.windows(2) {
            if let Some(index) = remaining.iter().position(|other| *other == pair) {
                remaining.remove(index);
                common += 1;
            }
        }
        let total = left.len().saturating_sub(1) + right.len().saturating_sub(1);
        if total == 0 {
            0.0
        } else {
            2.0 * common as f64 / total as f64
        }
    }

    #[test]
    fn cached_shapes_preserve_bigram_multiplicity_and_empty_scores() {
        let shapes: Vec<Vec<String>> = [
            vec![],
            vec!["a"],
            vec!["a", "b", "a", "b"],
            vec!["a", "b", "a"],
        ]
        .iter()
        .map(|s| s.iter().map(|v| (*v).to_owned()).collect())
        .collect();
        for left in &shapes {
            for right in &shapes {
                let expected = reference_similarity(left, right);
                let mut symbols = HashMap::new();
                assert_eq!(
                    similarity(
                        &Shape::new(left, &mut symbols),
                        &Shape::new(right, &mut symbols)
                    ),
                    expected
                );
            }
        }
    }
}

#[cfg(test)]
mod reference;

#[cfg(test)]
mod equivalence_tests {
    use super::{candidates, reference};
    use crate::{
        features::{configuration::models::DuplicateSettings, parsing::parsers::parse_script},
        shared::models::Outcome,
    };

    #[test]
    fn optimized_candidates_match_original_locations_scores_and_order() -> Outcome<()> {
        let mut text =
            String::from("extends RefCounted\nfunc one(input: int) -> int:\n\tvar total = input\n");
        for i in 0..16 {
            text.push_str(&format!("\ttotal = (total + {i}) * 2 - input\n"));
        }
        text.push_str("\treturn total\n");
        let variations = [
            text.clone(),
            text.replace("one", "two"),
            text.replace("input", "value"),
            text.replace("+ 5", "+ 9"),
        ];
        let sources = variations
            .iter()
            .enumerate()
            .map(|(i, value)| parse_script(&format!("{i}.gd"), value.clone()))
            .collect::<Outcome<Vec<_>>>()?;
        let units: Vec<_> = sources
            .iter()
            .flat_map(|s| s.functions.iter().map(move |f| (s, f)))
            .collect();
        let settings = DuplicateSettings::default();
        assert_eq!(
            format!("{:?}", candidates(&units, &settings, &[], &[])),
            format!("{:?}", reference::candidates(&units, &settings))
        );
        Ok(())
    }
}
