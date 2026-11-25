use crate::{
    features::configuration::{
        helpers::{inside, safe_path},
        models::Project,
    },
    shared::{
        helpers::issue,
        models::{Corpus, Diagnostic, Reference, Source},
    },
};
use std::collections::{BTreeMap, BTreeSet};

pub fn check_dependencies(project: &Project, corpus: &Corpus, diagnostics: &mut Vec<Diagnostic>) {
    let sources: BTreeMap<_, _> = corpus
        .sources
        .iter()
        .chain(&corpus.excluded_sources)
        .map(|source| (source.path.as_str(), source))
        .collect();
    let mut classes: BTreeMap<&str, &Source> = BTreeMap::new();
    for source in corpus.sources.iter().chain(&corpus.excluded_sources) {
        if let Some(name) = &source.class_name
            && let Some(previous) = classes.insert(name, source)
        {
            diagnostics.push(issue(
                &source.path,
                1,
                1,
                "architecture/class-unique",
                name,
                &previous.path,
                "One globally unique class_name",
            ));
        }
    }
    let mut used = BTreeSet::new();
    if let Some(project_source) = sources.get("project.godot") {
        for reference in &project_source.references {
            if let Some(name) = reference.item.strip_prefix("autoload:")
                && let Some(path) = reference.target.strip_prefix("res://")
                && let Some(target) = sources.get(path)
                && classes.insert(name, target).is_some()
            {
                diagnostics.push(issue(
                    "project.godot",
                    reference.line,
                    reference.column,
                    "architecture/class-unique",
                    name,
                    name,
                    "Autoload and class_name symbols must be unique",
                ));
            }
        }
    }
    for source in &corpus.sources {
        for reference in &source.references {
            if reference.dynamic {
                check_dynamic(
                    project,
                    corpus,
                    source,
                    reference,
                    &sources,
                    diagnostics,
                    &mut used,
                );
            } else {
                check_reference(project, corpus, source, reference, &sources, diagnostics);
            }
        }
        for (name, line, column) in &source.class_uses {
            if let Some(target) = classes.get(name.as_str()) {
                check_direction(source, target, *line, *column, name, diagnostics);
            }
        }
    }
    for (index, entry) in project.settings.dynamic_loads.iter().enumerate() {
        if !used.contains(&index) {
            diagnostics.push(issue(
                &entry.path,
                1,
                1,
                "architecture/dynamic-load-stale",
                &entry.item,
                "Unused dynamic load declaration",
                "Remove stale declaration",
            ));
        }
    }
}

fn check_dynamic(
    project: &Project,
    corpus: &Corpus,
    source: &Source,
    reference: &Reference,
    sources: &BTreeMap<&str, &Source>,
    diagnostics: &mut Vec<Diagnostic>,
    used: &mut BTreeSet<usize>,
) {
    let declaration = project
        .settings
        .dynamic_loads
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.path == source.path && entry.item == reference.item);
    let Some((index, declaration)) = declaration else {
        diagnostics.push(issue(
            &source.path,
            reference.line,
            reference.column,
            "architecture/dynamic-load",
            &reference.item,
            &reference.target,
            "Literal targets or a reasoned function-to-resource-root declaration",
        ));
        return;
    };
    used.insert(index);
    let concrete = reference.target.starts_with("user://");
    if concrete
        && (!declaration
            .roots
            .iter()
            .any(|root| inside(&reference.target, root))
            || (!reference.directory && !declaration.matches_extension(&reference.target)))
    {
        diagnostics.push(issue(
            &source.path,
            reference.line,
            reference.column,
            "architecture/dynamic-load-bound",
            &reference.item,
            &reference.target,
            "Load target within declared runtime roots and extension filter",
        ));
    }
    for root in &declaration.roots {
        let Some(root) = root.strip_prefix("res://") else {
            continue;
        };
        for path in &corpus.paths {
            if inside(path, root)
                && declaration.matches_extension(path)
                && let Some(target) = sources.get(path.as_str())
            {
                check_direction(
                    source,
                    target,
                    reference.line,
                    reference.column,
                    &reference.item,
                    diagnostics,
                );
            }
        }
    }
}

fn check_reference(
    project: &Project,
    corpus: &Corpus,
    source: &Source,
    reference: &Reference,
    sources: &BTreeMap<&str, &Source>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let target = if let Some(path) = reference.target.strip_prefix("res://") {
        path.split("::")
            .next()
            .unwrap_or(path)
            .trim_end_matches('/')
            .to_owned()
    } else if reference.target.starts_with("uid://") {
        if reference.item.starts_with("[gd_") {
            return;
        }
        corpus
            .uids
            .get(&reference.target)
            .cloned()
            .unwrap_or_default()
    } else {
        return;
    };
    if safe_path(&target).is_err() || !corpus.paths.contains(&target) {
        if !reference.required {
            return;
        }
        diagnostics.push(issue(
            &source.path,
            reference.line,
            reference.column,
            "architecture/reference",
            &reference.item,
            &reference.target,
            "Existing project-relative reference with exact path casing",
        ));
        return;
    }
    if reference.directory {
        for (path, target_source) in sources.range(target.as_str()..) {
            if inside(path, &target) {
                check_direction(
                    source,
                    target_source,
                    reference.line,
                    reference.column,
                    &reference.item,
                    diagnostics,
                );
            }
        }
    } else if let Some(target) = sources.get(target.as_str()) {
        check_direction(
            source,
            target,
            reference.line,
            reference.column,
            &reference.item,
            diagnostics,
        );
    } else {
        let mut target_source =
            crate::features::parsing::parsers::parse_text(&target, String::new());
        super::sources::identity(project, &mut target_source);
        check_direction(
            source,
            &target_source,
            reference.line,
            reference.column,
            &reference.item,
            diagnostics,
        );
    }
}

fn check_direction(
    source: &Source,
    target: &Source,
    line: usize,
    column: usize,
    item: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if source.path == "project.godot" && item == "[editor_plugins]" {
        return;
    }
    let forbidden = (source.layer == "shared"
        && ["feature", "app"].contains(&target.layer.as_str()))
        || (["feature", "editor"].contains(&source.layer.as_str()) && target.layer == "app")
        || (!source.testing && target.testing)
        || (!source.testing
            && !["editor", "tooling"].contains(&source.layer.as_str())
            && ["editor", "tooling"].contains(&target.layer.as_str()));
    if forbidden {
        diagnostics.push(issue(&source.path, line, column, "architecture/dependency-direction", item,
            &target.path, "shared -> shared; features -> shared/features; runtime cannot load testing/editor/tooling"));
    }
}
