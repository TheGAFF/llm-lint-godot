use super::{
    dependencies::check_dependencies,
    document_content::GitBaseline,
    roles::check_roles,
    sources::{ScriptTrees, identity, parse_corpus_scripts, parsed_corpus},
};
use crate::{
    features::configuration::{
        helpers::{inside, target_paths},
        models::Project,
    },
    features::parsing::parsers::parse_text,
    shared::{
        helpers::issue,
        models::{ArchitectureReport, Corpus, Diagnostic, Outcome},
    },
};
use std::collections::{BTreeMap, BTreeSet};

const EMPTY_DIRECTORY_RULE: &str = "architecture/empty-directory";

pub fn analyze_project(project: &Project, targets: &[String]) -> Outcome<ArchitectureReport> {
    let (corpus, trees) = parsed_corpus(project)?;
    analyze_parsed_corpus(project, &corpus, &trees, targets, None)
}

pub fn analyze_corpus(
    project: &Project,
    corpus: &Corpus,
    targets: &[String],
) -> Outcome<ArchitectureReport> {
    let trees = parse_corpus_scripts(corpus)?;
    analyze_parsed_corpus(project, corpus, &trees, targets, None)
}

pub(crate) fn analyze_parsed_corpus(
    project: &Project,
    corpus: &Corpus,
    trees: &ScriptTrees,
    targets: &[String],
    doc_baseline: Option<&mut GitBaseline>,
) -> Outcome<ArchitectureReport> {
    let targets = target_paths(project, targets)?;
    let owners: BTreeSet<_> = corpus
        .sources
        .iter()
        .chain(&corpus.documents)
        .filter(|source| targets.iter().any(|target| inside(&source.path, target)))
        .map(|source| source.owner.as_str())
        .collect();
    let mut diagnostics = Vec::new();
    for source in &corpus.sources {
        diagnostics.extend(source.diagnostics.clone());
        check_roles(project, source, corpus, &mut diagnostics)?;
    }
    super::documents::check(corpus, &mut diagnostics)?;
    let document_checks =
        super::document_content::check(project, corpus, &targets, &mut diagnostics)?;
    super::file_roles::check_collisions(corpus, &mut diagnostics);
    check_dependencies(project, corpus, &mut diagnostics);
    super::contracts::check_contracts(project, corpus, trees, &mut diagnostics)?;
    super::identifiers::check(corpus, trees, &mut diagnostics)?;
    conflicts(project, corpus, &mut diagnostics)?;
    super::bundles::check(project, corpus, &mut diagnostics)?;
    super::suppressions::check(project, corpus, &mut diagnostics)?;
    check_empty_directories(project, corpus, &mut diagnostics);
    super::bundles::apply_exclusions(project, &mut diagnostics);
    exceptions(project, &mut diagnostics);
    if !targets.is_empty() {
        retain_targets(
            project,
            corpus,
            &owners,
            &targets,
            &document_checks.dependents,
            &mut diagnostics,
        );
    }
    diagnostics.sort_by(|left, right| {
        (&left.path, left.line, left.column, &left.rule, &left.item).cmp(&(
            &right.path,
            right.line,
            right.column,
            &right.rule,
            &right.item,
        ))
    });
    diagnostics.dedup_by(|right, left| {
        left.path == right.path
            && left.line == right.line
            && left.column == right.column
            && left.rule == right.rule
            && left.item == right.item
            && left.actual == right.actual
    });
    apply_levels(project, &mut diagnostics)?;
    document_checks.explain(&mut diagnostics, doc_baseline);
    let mut counts = BTreeMap::new();
    for diagnostic in &diagnostics {
        *counts.entry(diagnostic.rule.clone()).or_insert(0) += 1;
    }
    Ok(ArchitectureReport {
        version: 1,
        mode: "complete",
        scanned_files: corpus.sources.len() + corpus.documents.len(),
        violating_files: diagnostics
            .iter()
            .map(|diagnostic| &diagnostic.path)
            .collect::<BTreeSet<_>>()
            .len(),
        errors: diagnostics
            .iter()
            .filter(|item| item.severity == "error")
            .count(),
        warnings: diagnostics
            .iter()
            .filter(|item| item.severity == "warning")
            .count(),
        counts,
        diagnostics,
    })
}

fn apply_levels(project: &Project, diagnostics: &mut Vec<Diagnostic>) -> Outcome<()> {
    let registered = crate::features::configuration::rules::catalog()?;
    if let Some(diagnostic) = diagnostics.iter().find(|diagnostic| {
        !registered
            .iter()
            .any(|definition| definition["id"] == diagnostic.rule)
    }) {
        return Err(format!("Unregistered diagnostic rule: {}", diagnostic.rule).into());
    }
    diagnostics.retain_mut(|diagnostic| {
        let level = crate::features::configuration::rules::level(
            &project.settings.rules,
            &project.settings.overrides,
            &diagnostic.rule,
            &diagnostic.path,
            registered
                .iter()
                .find(|definition| definition["id"] == diagnostic.rule)
                .and_then(|definition| definition["default"].as_str())
                .unwrap_or("error"),
        );
        diagnostic.severity = if level == "warn" { "warning" } else { "error" }.into();
        level != "off"
    });
    Ok(())
}

fn retain_targets(
    project: &Project,
    corpus: &Corpus,
    owners: &BTreeSet<&str>,
    targets: &[String],
    dependent_documents: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let directory_owners: BTreeSet<_> = targets
        .iter()
        .filter(|target| project.root.join(target).is_dir())
        .map(|target| directory_owner(project, target))
        .collect();
    diagnostics.retain(|diagnostic| {
        if diagnostic.rule == EMPTY_DIRECTORY_RULE
            || (diagnostic.rule == "architecture/exception-stale"
                && diagnostic.actual == EMPTY_DIRECTORY_RULE)
        {
            let owner = directory_owner(project, &diagnostic.path);
            return owners.contains(owner.as_str())
                || directory_owners.contains(&owner)
                || targets
                    .iter()
                    .any(|target| inside(&diagnostic.path, target));
        }
        super::bundles::include_target(project, diagnostic, owners, targets)
            || corpus
                .sources
                .iter()
                .chain(&corpus.documents)
                .any(|source| {
                    source.path == diagnostic.path
                        && (owners.contains(source.owner.as_str())
                            || dependent_documents.contains(&source.path)
                            || targets.iter().any(|target| inside(&source.path, target)))
                })
    });
}

fn directory_owner(project: &Project, path: &str) -> String {
    let mut source = parse_text(&format!("{path}/directory.gd"), String::new());
    identity(project, &mut source);
    source.owner
}

fn check_empty_directories(project: &Project, corpus: &Corpus, diagnostics: &mut Vec<Diagnostic>) {
    let settings = &project.settings;
    for path in &corpus.empty_directories {
        if settings
            .source_roots
            .iter()
            .chain(&settings.plugin_roots)
            .chain(&settings.tooling_roots)
            .any(|root| inside(path, root))
            && !settings
                .excluded
                .iter()
                .any(|entry| inside(path, &entry.path))
        {
            diagnostics.push(issue(
                path,
                1,
                1,
                EMPTY_DIRECTORY_RULE,
                "<directory>",
                "Empty directory",
                "Remove this empty directory, or add an exact exception explaining its planned use.",
            ));
        }
    }
}

fn conflicts(project: &Project, corpus: &Corpus, diagnostics: &mut Vec<Diagnostic>) -> Outcome<()> {
    let roles = crate::shared::helpers::registry()?;
    for source in &corpus.sources {
        if super::bundles::enabled(project, source) {
            continue;
        }
        let Some(stem) = source.path.strip_suffix(".gd") else {
            continue;
        };
        for plural in roles
            .iter()
            .map(|role| role.1.as_str())
            .chain(std::iter::once("helpers"))
        {
            if !stem.ends_with(&format!("_{plural}"))
                || source.testing
                || !stem
                    .rsplit_once('/')
                    .is_some_and(|(parent, _)| parent == source.owner)
            {
                continue;
            }
            let directory = format!("{}/{plural}", source.owner);
            if corpus.paths.contains(&directory) {
                diagnostics.push(issue(
                    &source.path,
                    1,
                    1,
                    "architecture/role-layout-conflict",
                    "<script>",
                    &directory,
                    "Collection file XOR role directory",
                ));
            }
        }
    }
    Ok(())
}

fn exceptions(project: &Project, diagnostics: &mut Vec<Diagnostic>) {
    for exception in &project.settings.exceptions {
        let count = diagnostics.len();
        diagnostics.retain(|diagnostic| {
            !(diagnostic.path == exception.path
                && diagnostic.rule == exception.rule
                && diagnostic.item == exception.item)
        });
        if count == diagnostics.len() {
            diagnostics.push(issue(
                &exception.path,
                1,
                1,
                "architecture/exception-stale",
                &exception.item,
                &exception.rule,
                "Remove unused exact exception",
            ));
        }
    }
}
