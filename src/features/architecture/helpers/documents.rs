use super::file_roles::{FileRole, catalog};
use crate::{
    features::configuration::{
        models::{DocumentationExclusion, Project},
        rules::matches,
    },
    shared::{
        helpers::{issue, snake_case},
        models::{Corpus, Diagnostic, Outcome, Source},
    },
};
use std::collections::{BTreeMap, BTreeSet};

pub fn validity(exclusions: &[DocumentationExclusion]) -> Outcome<()> {
    for entry in exclusions {
        if entry.reason.trim().is_empty()
            || entry.includes.is_empty()
            || entry.includes.iter().any(|pattern| {
                pattern.is_empty()
                    || pattern.starts_with('/')
                    || pattern.contains(['\\', ':', '[', ']', '{', '}'])
                    || pattern
                        .split('/')
                        .any(|part| part.is_empty() || part == "." || part == "..")
            })
        {
            return Err("Documentation exclusions require a nonempty reason and project-relative globs using *, ** or ?".into());
        }
    }
    Ok(())
}

pub fn exempt(project: &Project, path: &str) -> bool {
    let filename = path.rsplit('/').next().unwrap_or(path);
    path == "README.md"
        || ["AGENTS.md", "LICENSE.md", "COPYING.md", "NOTICE.md"].contains(&filename)
        || project
            .settings
            .documentation_exclusions
            .iter()
            .any(|entry| entry.includes.iter().any(|pattern| matches(pattern, path)))
}

pub fn check(corpus: &Corpus, diagnostics: &mut Vec<Diagnostic>) -> Outcome<()> {
    let roles: Vec<_> = catalog()?
        .into_iter()
        .filter(|role| role.extensions.iter().any(|ext| ext == "md"))
        .collect();
    let mut destinations: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for document in &corpus.documents {
        if let Some(destination) = check_document(document, &roles, diagnostics) {
            destinations
                .entry(destination)
                .or_default()
                .insert(document.path.clone());
        }
    }
    for (destination, sources) in destinations {
        for source in &sources {
            if sources.len() > 1
                || corpus
                    .paths
                    .iter()
                    .any(|path| path != source && path.eq_ignore_ascii_case(&destination))
            {
                diagnostics.push(document_issue(source, "architecture/document-collision", &destination,
                    "Suggested document destination exists or is shared; choose a unique subject without overwriting it"));
            }
        }
    }
    Ok(())
}

fn check_document(
    source: &Source,
    roles: &[FileRole],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    let registered = source.layer != "unregistered";
    if !registered {
        diagnostics.push(document_issue(
            &source.path,
            "architecture/feature-layout",
            "Document has no registered owner",
            "features/<owner>, shared, app or a registered plugin/tool owner",
        ));
    }
    if source.path.split('/').any(|part| {
        let stem = part
            .strip_suffix(".md")
            .or_else(|| part.strip_suffix(".MD"))
            .unwrap_or(part);
        !snake_case(stem)
    }) || !source.path.ends_with(".md")
    {
        diagnostics.push(document_issue(
            &source.path,
            "architecture/file-name",
            &source.path,
            "lowercase snake_case paths with .md extension",
        ));
    }
    let local = source
        .path
        .strip_prefix(&format!("{}/", source.owner))
        .unwrap_or(&source.path);
    let filename = local.rsplit('/').next().unwrap_or(local);
    let stem = filename.rsplit_once('.').map_or(filename, |(stem, _)| stem);
    let selected = roles
        .iter()
        .find(|role| stem.ends_with(&format!("_{}", role.role)))
        .or_else(|| {
            roles
                .iter()
                .find(|role| crate::features::configuration::helpers::inside(local, &role.folder))
        });
    let Some(role) = selected else {
        let choices = roles
            .iter()
            .map(|role| format!("{}/<subject>_{}.md", role.folder, role.role))
            .collect::<Vec<_>>()
            .join("; ");
        diagnostics.push(document_issue(
            &source.path,
            "architecture/document-role",
            "Document role is unknown",
            &choices,
        ));
        return None;
    };
    let destination = destination(source, local, stem, role, roles);
    let expected = if registered {
        destination.as_str()
    } else {
        "<registered_owner>/docs/<role-folder>/<subject>_<role>.md"
    };
    if !crate::features::configuration::helpers::inside(local, &role.folder) {
        diagnostics.push(document_issue(
            &source.path,
            "architecture/role-placement",
            &source.path,
            expected,
        ));
    }
    let valid = stem
        .strip_suffix(&format!("_{}", role.role))
        .is_some_and(|subject| {
            snake_case(subject) && subject.chars().any(|ch| ch.is_ascii_lowercase())
        })
        && filename.ends_with(".md");
    if !valid {
        diagnostics.push(document_issue(
            &source.path,
            "architecture/role-filename",
            filename,
            expected,
        ));
    }
    registered
        .then_some(destination)
        .filter(|target| target != &source.path)
}

fn destination(
    source: &Source,
    local: &str,
    stem: &str,
    role: &FileRole,
    roles: &[FileRole],
) -> String {
    let subject = normalize(
        stem.strip_suffix(&format!("_{}", role.role))
            .unwrap_or(stem),
    );
    let subject = if subject.chars().any(|ch| ch.is_ascii_lowercase()) {
        subject
    } else {
        format!("custom_{subject}").trim_end_matches('_').to_owned()
    };
    let directory = local
        .rsplit_once('/')
        .map_or("", |(directory, _)| directory);
    let directory = roles
        .iter()
        .find_map(|entry| {
            directory
                .strip_prefix(&entry.folder)
                .filter(|tail| tail.is_empty() || tail.starts_with('/'))
        })
        .unwrap_or_else(|| {
            directory
                .strip_prefix("docs")
                .filter(|tail| tail.is_empty() || tail.starts_with('/'))
                .unwrap_or(directory)
        });
    [
        source.owner.as_str(),
        role.folder.as_str(),
        normalize(directory).as_str(),
        &format!("{subject}_{}.md", role.role),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("/")
}

fn normalize(value: &str) -> String {
    value
        .split('/')
        .map(|part| {
            part.chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() {
                        ch.to_ascii_lowercase()
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
                .split('_')
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>()
                .join("_")
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

fn document_issue(path: &str, rule: &str, actual: &str, expected: &str) -> Diagnostic {
    issue(path, 1, 1, rule, "<document>", actual, expected)
}
