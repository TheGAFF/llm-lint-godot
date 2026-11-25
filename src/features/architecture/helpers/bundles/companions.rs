use super::{COMPANION, CONFIGURATION, enabled, location, parent};
use crate::{
    features::configuration::{
        helpers::{inside, safe_path},
        models::{BundleCompanions, Project},
    },
    shared::{
        helpers::{issue, registry},
        models::{Corpus, Diagnostic, Outcome, Reference, Source},
    },
};
use std::collections::{BTreeMap, BTreeSet};

pub fn validate(declarations: &[BundleCompanions], owners: &[String]) -> Outcome<()> {
    let mut anchors = BTreeSet::new();
    let mut companions = BTreeMap::new();
    for declaration in declarations {
        let anchor = declaration.anchor.as_str();
        safe_path(anchor)?;
        let owner = owners
            .iter()
            .find(|owner| anchor != owner.as_str() && inside(anchor, owner))
            .ok_or_else(|| format!("Bundle anchor has no enabled owner: {anchor}"))?;
        if declaration.paths.is_empty() || !anchors.insert(anchor) {
            return Err(format!("Empty or repeated bundle declaration for {anchor}").into());
        }
        for path in &declaration.paths {
            safe_path(path)?;
            if path == anchor || !inside(path, owner) {
                return Err(format!("Companion {path} and anchor {anchor} need one owner").into());
            }
            if let Some(previous) = companions.insert(path.as_str(), anchor) {
                return Err(
                    format!("Companion {path} is declared for {previous} and {anchor}").into(),
                );
            }
        }
    }
    for path in companions.keys() {
        let mut current = *path;
        let mut visited = BTreeSet::new();
        while let Some(anchor) = companions.get(current) {
            if !visited.insert(current) {
                return Err(format!("Cyclic companion declaration involving {path}").into());
            }
            current = anchor;
        }
    }
    Ok(())
}

pub fn check(project: &Project, corpus: &Corpus, diagnostics: &mut Vec<Diagnostic>) -> Outcome<()> {
    let sources: BTreeMap<_, _> = corpus
        .sources
        .iter()
        .map(|source| (source.path.as_str(), source))
        .collect();
    let declared = check_declarations(project, &sources, diagnostics);
    let classes: BTreeMap<_, _> = corpus
        .sources
        .iter()
        .chain(&corpus.excluded_sources)
        .filter_map(|source| Some((source.class_name.as_deref()?, source.path.as_str())))
        .collect();
    let mut functional = registry()?
        .into_iter()
        .flat_map(|role| [role.0, role.1])
        .map(|role| format!("_{role}"))
        .collect::<Vec<_>>();
    functional.push("_helpers".into());
    let mut dependencies = BTreeMap::<&str, Use>::new();
    for source in corpus.sources.iter().chain(&corpus.excluded_sources) {
        if source.testing {
            continue;
        }
        let attached = script_attachments(source);
        for reference in &source.references {
            let Some(path) = resolve(reference, corpus, &sources) else {
                continue;
            };
            let evidence = reference_evidence(source, reference, path, &attached, &functional);
            insert(&mut dependencies, path, source, evidence);
        }
        for (class, _, _) in &source.class_uses {
            if let Some(path) = classes
                .get(class.as_str())
                .filter(|path| sources.contains_key(**path))
            {
                let evidence = is_functional(path, &functional).then_some(Evidence::ClassUse);
                insert(&mut dependencies, path, source, evidence);
            }
        }
    }
    let declared_anchors: BTreeSet<_> = project
        .settings
        .bundles
        .companions
        .iter()
        .map(|entry| entry.anchor.as_str())
        .collect();
    for (&path, site) in &dependencies {
        let Some(target) = sources.get(path) else {
            continue;
        };
        if let Use::Exclusive(anchor, Some(evidence)) = site
            && target.owner == anchor.owner
            && enabled(project, anchor)
            && !target.testing
            && !declared.contains(path)
            && (!matches!(evidence, Evidence::ScriptPath | Evidence::ClassUse)
                || script_has_anchor(anchor, &declared, &declared_anchors, &dependencies))
        {
            check_location(anchor, target, evidence.description(), diagnostics);
        }
    }
    Ok(())
}

enum Use<'a> {
    Exclusive(&'a Source, Option<Evidence>),
    Shared,
}

#[derive(Clone, Copy)]
enum Evidence {
    Attachment,
    Asset,
    ScriptPath,
    ClassUse,
}

impl Evidence {
    fn description(self) -> &'static str {
        match self {
            Self::Attachment => "resolved scene or resource script attachment",
            Self::Asset => "resolved scene or resource asset reference",
            Self::ScriptPath => "resolved script resource reference",
            Self::ClassUse => "resolved script class use",
        }
    }
}

fn script_has_anchor<'a>(
    mut script: &'a Source,
    declared: &BTreeSet<&str>,
    anchors: &BTreeSet<&str>,
    dependencies: &BTreeMap<&'a str, Use<'a>>,
) -> bool {
    let mut visited = BTreeSet::new();
    while visited.insert(script.path.as_str()) {
        if declared.contains(script.path.as_str()) || anchors.contains(script.path.as_str()) {
            return true;
        }
        let Some(Use::Exclusive(caller, Some(evidence))) = dependencies.get(script.path.as_str())
        else {
            return false;
        };
        if caller.owner != script.owner {
            return false;
        }
        if matches!(evidence, Evidence::Attachment) {
            return true;
        }
        script = caller;
    }
    false
}

fn insert<'a>(
    index: &mut BTreeMap<&'a str, Use<'a>>,
    path: &'a str,
    source: &'a Source,
    evidence: Option<Evidence>,
) {
    if path == source.path {
        return;
    }
    let usage = index
        .entry(path)
        .or_insert(Use::Exclusive(source, evidence));
    match usage {
        Use::Exclusive(anchor, kind) if anchor.path == source.path => *kind = kind.or(evidence),
        _ => *usage = Use::Shared,
    }
}

fn is_functional(path: &str, suffixes: &[String]) -> bool {
    path.strip_suffix(".gd")
        .is_some_and(|stem| suffixes.iter().any(|suffix| stem.ends_with(suffix)))
}

fn resolve<'a>(
    reference: &Reference,
    corpus: &'a Corpus,
    sources: &BTreeMap<&'a str, &'a Source>,
) -> Option<&'a str> {
    if reference.dynamic || reference.directory || reference.item.starts_with("[gd_") {
        return None;
    }
    let path = reference.target.strip_prefix("res://").map_or_else(
        || corpus.uids.get(&reference.target).map(String::as_str),
        |path| Some(path.split("::").next().unwrap_or(path)),
    )?;
    sources.get_key_value(path).map(|(&path, _)| path)
}

fn reference_evidence(
    source: &Source,
    reference: &Reference,
    target: &str,
    attachments: &BTreeSet<&str>,
    functional: &[String],
) -> Option<Evidence> {
    if source.path.ends_with(".gd") && is_functional(target, functional) {
        return Some(Evidence::ScriptPath);
    }
    if !matches!(
        source.path.rsplit_once('.').map(|(_, extension)| extension),
        Some("tscn" | "tres")
    ) || !reference.item.starts_with("[ext_resource ")
    {
        return None;
    }
    if target.ends_with(".gd") {
        return attribute(&reference.item, "id")
            .is_some_and(|id| attachments.contains(id))
            .then_some(Evidence::Attachment);
    }
    if matches!(
        target.rsplit_once('.').map(|(_, extension)| extension),
        Some("tscn" | "scn" | "tres" | "res" | "po" | "pot" | "translation")
    ) {
        return None;
    }
    Some(Evidence::Asset)
}

fn script_attachments(source: &Source) -> BTreeSet<&str> {
    if !source.path.ends_with(".tscn") && !source.path.ends_with(".tres") {
        return BTreeSet::new();
    }
    source
        .text
        .lines()
        .filter_map(|line| {
            let (property, value) = line.split_once('=')?;
            (property.trim() == "script")
                .then(|| {
                    value
                        .trim()
                        .strip_prefix("ExtResource(\"")?
                        .split('"')
                        .next()
                })
                .flatten()
        })
        .collect()
}

fn attribute<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.split_once(&format!(" {name}=\""))?.1.split('"').next()
}

fn check_declarations<'a>(
    project: &'a Project,
    sources: &BTreeMap<&str, &Source>,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeSet<&'a str> {
    let mut declared = BTreeSet::new();
    for entry in &project.settings.bundles.companions {
        let Some(anchor) = sources.get(entry.anchor.as_str()) else {
            stale(&entry.anchor, "Missing or unmaintained anchor", diagnostics);
            continue;
        };
        if !enabled(project, anchor) || !is_anchor(anchor) {
            stale(
                &entry.anchor,
                "Anchor must be a script, scene or resource in an enabled owner",
                diagnostics,
            );
            continue;
        }
        for path in &entry.paths {
            let Some(target) = sources
                .get(path.as_str())
                .filter(|target| target.owner == anchor.owner)
            else {
                stale(
                    path,
                    "Missing, unmaintained or cross-owner companion",
                    diagnostics,
                );
                continue;
            };
            declared.insert(path.as_str());
            check_location(
                anchor,
                target,
                "explicit companion declaration",
                diagnostics,
            );
        }
    }
    declared
}

fn is_anchor(source: &Source) -> bool {
    matches!(
        source.path.rsplit_once('.').map(|(_, extension)| extension),
        Some("gd" | "tscn" | "scn" | "tres" | "res")
    )
}

fn stale(path: &str, reason: &str, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.push(issue(
        "llm-lint.godot.toml",
        1,
        1,
        CONFIGURATION,
        path,
        reason,
        "Use exact paths to existing maintained files",
    ));
}

fn check_location(
    anchor: &Source,
    target: &Source,
    evidence: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let directory = parent(&anchor.path);
    if inside(&target.path, directory) {
        return;
    }
    let file = target.path.rsplit('/').next().unwrap_or(&target.path);
    diagnostics.push(issue(
        &target.path,
        location(target),
        1,
        COMPANION,
        &anchor.path,
        &format!("{evidence} from {}", anchor.path),
        &format!("{directory}/{file} or within one of its subject subdirectories"),
    ));
}
