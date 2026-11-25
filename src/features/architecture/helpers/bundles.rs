mod companions;

use super::{file_roles, roles, sources::identity};
use crate::{
    features::{
        configuration::{
            helpers::{inside, safe_path},
            models::{Project, Settings},
        },
        parsing::parsers::parse_text,
    },
    shared::{
        helpers::{issue, registry},
        models::{Corpus, Diagnostic, Outcome, Source},
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub const PLACEMENT: &str = "architecture/bundle-placement";
pub const COMPANION: &str = "architecture/bundle-companion";
pub const SIZE: &str = "architecture/bundle-size";
pub const CONFIGURATION: &str = "architecture/bundle-configuration";

pub fn is_layout_rule(rule: &str) -> bool {
    [PLACEMENT, COMPANION, SIZE, CONFIGURATION].contains(&rule)
}

pub fn enabled(project: &Project, source: &Source) -> bool {
    project.settings.bundles.owners.contains(&source.owner)
}

pub fn extra_asset(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, extension)| {
        [
            "gdshader",
            "gdshaderinc",
            "shader",
            "po",
            "pot",
            "translation",
        ]
        .contains(&extension)
    })
}

pub fn native_locale_filename(file: &str) -> bool {
    let Some((stem, "po" | "translation")) = file.rsplit_once('.') else {
        return false;
    };
    let mut parts = stem.split('_');
    parts.next().is_some_and(|language| {
        (2..=3).contains(&language.len()) && language.bytes().all(|ch| ch.is_ascii_lowercase())
    }) && parts.all(|part| {
        (part.len() == 2 && part.bytes().all(|ch| ch.is_ascii_uppercase()))
            || (part.len() == 3 && part.bytes().all(|ch| ch.is_ascii_digit()))
            || (part.len() == 4
                && part.as_bytes()[0].is_ascii_uppercase()
                && part.as_bytes()[1..].iter().all(u8::is_ascii_lowercase))
    })
}

pub fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

pub fn location(source: &Source) -> usize {
    source
        .text
        .lines()
        .position(|line| {
            let content = line.trim_start();
            !content.is_empty() && !content.starts_with(['#', ';'])
        })
        .map_or(1, |index| index + 1)
}

pub fn validity(root: &Path, settings: &Settings) -> Outcome<()> {
    let bundles = &settings.bundles;
    if bundles.max_files == 0 {
        return Err("Bundle directory cap must be positive".into());
    }
    let mut owners = BTreeSet::new();
    for owner in &bundles.owners {
        safe_path(owner)?;
        let exact_directory = root
            .join(owner)
            .canonicalize()
            .is_ok_and(|resolved| resolved.is_dir() && resolved == root.join(owner));
        if !registered_owner(settings, owner) || !exact_directory || !owners.insert(owner) {
            return Err(format!("Invalid or repeated bundle owner: {owner}").into());
        }
    }
    let mut exclusions = BTreeSet::new();
    for entry in &bundles.exclusions {
        safe_path(&entry.path)?;
        if ![PLACEMENT, COMPANION, SIZE].contains(&entry.rule.as_str())
            || entry.reason.trim().is_empty()
            || !owners.iter().any(|owner| inside(&entry.path, owner))
            || !exclusions.insert((&entry.path, &entry.rule))
        {
            return Err(format!("Invalid or repeated bundle exclusion: {}", entry.path).into());
        }
    }
    companions::validate(&bundles.companions, &bundles.owners)
}

fn registered_owner(settings: &Settings, owner: &str) -> bool {
    settings
        .source_roots
        .iter()
        .filter_map(|root| owner.strip_prefix(root)?.strip_prefix('/'))
        .any(|local| {
            matches!(local, "app" | "shared")
                || local
                    .strip_prefix("features/")
                    .is_some_and(|feature| !feature.is_empty() && !feature.contains('/'))
        })
        || settings
            .plugin_roots
            .iter()
            .chain(&settings.tooling_roots)
            .any(|root| root == owner)
}

pub fn check(project: &Project, corpus: &Corpus, diagnostics: &mut Vec<Diagnostic>) -> Outcome<()> {
    if project.settings.bundles.owners.is_empty() {
        return Ok(());
    }
    let mut reserved: BTreeSet<_> = file_roles::catalog()?
        .into_iter()
        .map(|role| role.folder.split('/').next().unwrap_or("").to_owned())
        .collect();
    reserved.extend(roles::STRUCTURAL.iter().map(|(role, _)| (*role).to_owned()));
    reserved.extend(registry()?.into_iter().map(|role| role.1));
    reserved.insert("helpers".into());
    let mut counts = BTreeMap::<&str, usize>::new();
    for source in corpus
        .sources
        .iter()
        .filter(|source| enabled(project, source))
    {
        count_and_check(project, source, &reserved, &mut counts, diagnostics);
    }
    let max = project.settings.bundles.max_files;
    for (directory, count) in counts.into_iter().filter(|(_, count)| *count > max) {
        diagnostics.push(issue(
            directory,
            1,
            1,
            SIZE,
            "<directory>",
            &format!("{count} directly contained Godot files"),
            &format!("At most {max} files; split this directory by subject"),
        ));
    }
    companions::check(project, corpus, diagnostics)
}

fn count_and_check<'a>(
    project: &Project,
    source: &'a Source,
    reserved: &BTreeSet<String>,
    counts: &mut BTreeMap<&'a str, usize>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    *counts.entry(parent(&source.path)).or_default() += 1;
    let local = source
        .path
        .strip_prefix(&source.owner)
        .unwrap_or(&source.path);
    let local = local.trim_start_matches('/');
    if (local == "plugin.cfg" && project.settings.plugin_roots.contains(&source.owner))
        || project
            .settings
            .entrypoints
            .iter()
            .any(|entry| entry.path == source.path)
    {
        return;
    }
    let file = local.rsplit('/').next().unwrap_or(local);
    let missing_testing_directory =
        !source.testing && file.strip_suffix(".gd").is_some_and(roles::is_test_stem);
    if local
        .split_once('/')
        .is_none_or(|(folder, _)| reserved.contains(folder))
        || missing_testing_directory
    {
        let testing = if missing_testing_directory || source.testing {
            "testing/"
        } else {
            ""
        };
        diagnostics.push(issue(
            &source.path,
            location(source),
            1,
            PLACEMENT,
            "<file>",
            &source.path,
            &format!("{}/<subject>/{testing}{file}", source.owner),
        ));
    }
}

pub fn apply_exclusions(project: &Project, diagnostics: &mut Vec<Diagnostic>) {
    // Evaluate every exclusion against the same findings so nested exclusions cannot conceal
    // a stale directive or depend on their ordering in the configuration.
    let exclusions = &project.settings.bundles.exclusions;
    let mut used = BTreeSet::new();
    diagnostics.retain(|diagnostic| {
        let exclusion = exclusions
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                diagnostic.rule == entry.rule
                    && if entry.rule == SIZE {
                        diagnostic.path == entry.path
                    } else {
                        inside(parent(&diagnostic.path), &entry.path)
                    }
            })
            .max_by_key(|(_, entry)| entry.path.len());
        exclusion.is_none_or(|(index, _)| {
            used.insert(index);
            false
        })
    });
    for (index, entry) in exclusions.iter().enumerate() {
        if !used.contains(&index) {
            diagnostics.push(issue(
                &entry.path,
                1,
                1,
                CONFIGURATION,
                &entry.rule,
                "Unused directory exclusion",
                "Remove the stale bundle exclusion",
            ));
        }
    }
}

pub fn include_target(
    project: &Project,
    diagnostic: &Diagnostic,
    owners: &BTreeSet<&str>,
    targets: &[String],
) -> bool {
    if !is_layout_rule(&diagnostic.rule) {
        return false;
    }
    if diagnostic.path == "llm-lint.godot.toml" {
        return true;
    }
    if targets
        .iter()
        .any(|target| inside(target, &diagnostic.path))
    {
        return true;
    }
    let mut source = parse_text(
        &format!("{}/bundle_target.gd", diagnostic.path),
        String::new(),
    );
    identity(project, &mut source);
    owners.contains(source.owner.as_str())
}
