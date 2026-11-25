use super::{Access, Index, Kind, MemberId, Usage, scenes::Scenes, visit};
use crate::{
    features::{
        configuration::models::UnusedSettings,
        parsing::helpers::column,
        unused::{Finding, Gap, Kind as IssueKind},
    },
    shared::models::{Corpus, Outcome, Source},
};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::Node;

#[derive(Default)]
pub(super) struct Audit<'a> {
    pub(super) used: BTreeSet<MemberId<'a>>,
    names: BTreeMap<String, BTreeSet<Access>>,
    pub(super) locals: BTreeMap<(&'a str, usize), Finding>,
    pub(super) gaps: Vec<Gap>,
}

impl Audit<'_> {
    pub(super) fn named(&mut self, name: &str, access: Access) {
        self.names.entry(name.into()).or_default().insert(access);
    }
}

pub(crate) fn audit_symbols(
    corpus: &Corpus,
    reachable: &BTreeSet<&str>,
    settings: &UnusedSettings,
) -> Outcome<(Vec<Finding>, Vec<Gap>)> {
    let sources = corpus.sources.iter().chain(&corpus.excluded_sources);
    let mut trees = BTreeMap::new();
    for source in sources
        .clone()
        .filter(|source| source.path.ends_with(".gd") && reachable.contains(source.path.as_str()))
    {
        trees.insert(source.path.clone(), super::super::tree(&source.text)?);
    }
    let scenes = Scenes::new(corpus);
    let index = Index::build(corpus, &trees, &scenes)?;
    let mut usage = Usage {
        audit: Some(Audit::default()),
        ..Usage::default()
    };
    let mut discarded = Vec::new();
    for (id, class) in index
        .classes
        .iter()
        .enumerate()
        .filter(|(_, class)| !class.body.has_error())
    {
        visit::script(&index, &mut usage, &mut discarded, id, class);
    }
    scenes.expose_reachable(&index, &mut usage, Some(reachable));
    let Some(mut audit) = usage.audit else {
        return Err("Missing GDScript audit state".into());
    };
    audit.used.extend(usage.external);
    for (id, _) in index.classes.iter().enumerate() {
        index.audit_overrides(id, &mut audit.used);
    }
    for source in sources.filter(|source| reachable.contains(source.path.as_str())) {
        if matches!(source.path.rsplit('.').next(), Some("tres" | "tscn")) {
            serialized_names(&source.text, &mut audit);
        }
    }
    let mut findings = member_findings(&index, &audit);
    findings.extend(audit.locals.into_values());
    findings.retain(|finding| {
        !settings
            .keep
            .iter()
            .any(|entry| entry.path == finding.path && entry.item.as_ref() == finding.item.as_ref())
    });
    Ok((findings, audit.gaps))
}

fn member_findings(index: &Index<'_>, audit: &Audit<'_>) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (id, class) in index
        .classes
        .iter()
        .enumerate()
        .filter(|(_, class)| class.lint && !class.body.has_error())
    {
        for (&name, member) in &class.members {
            if member.exposed || audit.used.contains(&(id, name)) {
                continue;
            }
            let kind = match member.kind {
                Kind::Method
                    if index.engine_callback(id, name) || test_hook(class.source, name) =>
                {
                    continue;
                }
                Kind::Method => IssueKind::Function,
                Kind::Variable | Kind::Constant => IssueKind::Variable,
                Kind::Other => continue,
            };
            if audit.names.get(name).is_some_and(|accesses| {
                accesses
                    .iter()
                    .any(|access| access.audit_matches(member.kind))
            }) {
                continue;
            }
            findings.push(at(
                class.source,
                member.name,
                kind,
                index.qualified(id, name),
            ));
        }
    }
    findings
}

fn test_hook(source: &Source, name: &str) -> bool {
    let test_path = source.testing
        || source
            .path
            .split('/')
            .any(|part| matches!(part, "test" | "tests"));
    test_path
        && (name.starts_with("test_")
            || matches!(
                name,
                "before_all" | "after_all" | "before_each" | "after_each" | "should_skip_script"
            ))
}

fn serialized_names(text: &str, audit: &mut Audit<'_>) {
    for line in text.lines().map(str::trim) {
        let methods = line
            .match_indices("\"method\"")
            .filter_map(|(offset, key)| {
                let value = line[offset + key.len()..].trim_start().strip_prefix(':')?;
                string_value(value)
            });
        for name in methods {
            audit.named(name, Access::Method);
        }
        if line.starts_with("tracks/")
            && let Some((key, value)) = line.split_once('=')
            && key.trim_end().ends_with("/path")
            && let Some(path) = value.trim().strip_prefix("NodePath(")
            && let Some((_, property)) = string_value(path).and_then(|path| path.split_once(':'))
        {
            audit.named(
                property.split(':').next().unwrap_or(property),
                Access::Variable,
            );
        }
    }
}

fn string_value(value: &str) -> Option<&str> {
    let value = value
        .trim_start()
        .trim_start_matches('&')
        .strip_prefix('"')?;
    value.split_once('"').map(|(name, _)| name)
}

pub(super) fn at(source: &Source, node: Node<'_>, kind: IssueKind, item: String) -> Finding {
    Finding {
        kind,
        path: source.path.clone(),
        item: Some(item),
        line: node.start_position().row + 1,
        column: column(node.start_byte(), &source.text),
        bytes: None,
        companions: Vec::new(),
    }
}
