use crate::{
    features::configuration::{helpers::inside, models::DuplicateException},
    shared::models::{Function, Source},
};
use std::collections::BTreeMap;

pub(super) struct Scope {
    targeted: Vec<bool>,
    groups: Vec<Vec<usize>>,
    group_count: usize,
}

impl Scope {
    pub(super) fn new(
        units: &[(&Source, &Function)],
        targets: &[String],
        exceptions: &[DuplicateException],
    ) -> Option<Self> {
        if targets.is_empty() {
            return None;
        }
        let mut by_path = BTreeMap::<&str, Vec<usize>>::new();
        let members = exceptions
            .iter()
            .enumerate()
            .flat_map(|(group, exception)| {
                exception.members.iter().map(move |member| (group, member))
            });
        // Include every function in these files. Redundancy can be decided by
        // an enclosing function with the same pair of paths.
        for (group, member) in members {
            let groups = by_path.entry(&member.path).or_default();
            if groups.last() != Some(&group) {
                groups.push(group);
            }
        }
        Some(Self {
            targeted: units
                .iter()
                .map(|(source, _)| targets.iter().any(|target| inside(&source.path, target)))
                .collect(),
            groups: units
                .iter()
                .map(|(source, _)| {
                    by_path
                        .get(source.path.as_str())
                        .cloned()
                        .unwrap_or_default()
                })
                .collect(),
            group_count: exceptions.len(),
        })
    }

    fn targeted(&self, unit: usize) -> bool {
        self.targeted[unit]
    }

    pub(super) fn includes(&self, left: usize, right: usize) -> bool {
        self.targeted(left)
            || self.targeted(right)
            || self.groups[left]
                .iter()
                .any(|group| self.groups[right].binary_search(group).is_ok())
    }

    pub(super) fn partners(&self, eligible: &[usize]) -> Partners<'_> {
        let mut groups = vec![Vec::new(); self.group_count];
        let mut targeted = Vec::new();
        for &unit in eligible {
            if self.targeted(unit) {
                targeted.push(unit);
            }
            for &group in &self.groups[unit] {
                groups[group].push(unit);
            }
        }
        Partners {
            scope: self,
            targeted,
            groups,
        }
    }
}

pub(super) struct Partners<'a> {
    scope: &'a Scope,
    targeted: Vec<usize>,
    groups: Vec<Vec<usize>>,
}

impl Partners<'_> {
    pub(super) fn after<'a>(
        &'a self,
        left: usize,
        all_after: &'a [usize],
        merged: &'a mut Vec<usize>,
    ) -> &'a [usize] {
        if self.scope.targeted(left) {
            return all_after;
        }
        let after = |units: &[usize]| units.partition_point(|&unit| unit <= left);
        let targeted_after = &self.targeted[after(&self.targeted)..];
        let groups = &self.scope.groups[left];
        if groups.is_empty() {
            return targeted_after;
        }
        merged.clear();
        merged.extend_from_slice(targeted_after);
        for &group in groups {
            let units = &self.groups[group];
            merged.extend_from_slice(&units[after(units)..]);
        }
        merged.sort_unstable();
        merged.dedup();
        merged
    }
}
