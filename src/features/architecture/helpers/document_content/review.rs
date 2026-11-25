use super::{Context, CurrentSources, Document, markdown, metadata, records, sources, validate};
use crate::{
    features::{architecture::helpers::corpus, configuration::models::Project},
    shared::{
        models::Outcome,
        paths::{guarded_path, replace_file},
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use toml_edit::{DocumentMut, value};

pub(crate) fn review(project: &Project, paths: &[String], reviewed: &str) -> Outcome<usize> {
    if !metadata::valid_date(reviewed) {
        return Err("--reviewed requires a valid non-future YYYY-MM-DD UTC date".into());
    }
    let paths = requested_paths(paths)?;
    let inventory = corpus(project)?;
    let mut context = Context {
        project,
        corpus: &inventory,
        hashes: BTreeMap::new(),
    };
    let mut changes = Vec::new();
    let mut diagnostics = Vec::new();
    for path in &paths {
        let source = inventory
            .documents
            .iter()
            .find(|document| &document.path == path)
            .ok_or_else(|| format!("Not a maintained document: {path}"))?;
        let document =
            Document::parse(source).ok_or_else(|| format!("No documentation role for {path}"))?;
        let declarations = markdown::declarations(&document);
        let current = validate(
            &mut context,
            &document,
            &declarations,
            true,
            &mut diagnostics,
        )?;
        let snapshot = match current {
            Some(CurrentSources::Complete(snapshot)) => Some(snapshot),
            _ => None,
        };
        let fingerprint = snapshot.as_ref().map(sources::Snapshot::fingerprint);
        let replacement = replacement(&document, reviewed, fingerprint.as_deref())?;
        changes.push((path, source.text.clone(), replacement, snapshot));
    }
    if !diagnostics.is_empty() {
        let messages = diagnostics
            .iter()
            .map(|diagnostic| {
                format!(
                    "{}:{} {}: {} -> {}",
                    diagnostic.path,
                    diagnostic.line,
                    diagnostic.rule,
                    diagnostic.actual,
                    diagnostic.expected
                )
            })
            .collect::<Vec<_>>();
        return Err(format!(
            "Review rejected; no files changed:\n{}",
            messages.join("\n")
        )
        .into());
    }
    for (path, original, _, _) in &changes {
        if fs::read_to_string(guarded_path(&project.root, Path::new(path))?)? != *original {
            return Err(format!("Document changed during review; no files changed: {path}").into());
        }
    }
    let mut updated = 0;
    for (path, original, replacement, snapshot) in changes {
        let record_changed = if let Some(snapshot) = snapshot {
            records::write(project, path, &snapshot)?
        } else {
            false
        };
        let document_changed = original != replacement;
        if document_changed {
            replace_file(
                &guarded_path(&project.root, Path::new(path))?,
                replacement.as_bytes(),
            )?;
        }
        if document_changed || record_changed {
            updated += 1;
        }
    }
    Ok(updated)
}

fn requested_paths(paths: &[String]) -> Outcome<BTreeSet<String>> {
    if paths.is_empty() {
        return Err("docs review requires explicit Markdown file paths".into());
    }
    let mut requested = BTreeSet::new();
    for path in paths {
        if !sources::safe(path) || path.contains(['*', '?']) || !path.ends_with(".md") {
            return Err(format!(
                "docs review accepts only explicit project-relative .md paths: {path}"
            )
            .into());
        }
        if !requested.insert(path.clone()) {
            return Err(format!("Repeated review path: {path}").into());
        }
    }
    Ok(requested)
}

fn replacement(
    document: &Document<'_>,
    reviewed: &str,
    fingerprint: Option<&str>,
) -> Outcome<String> {
    if document.metadata.error.is_some() {
        return Ok(document.source.text.clone());
    }
    let mut values = if document.metadata.present {
        document.metadata.content.parse::<DocumentMut>()?
    } else {
        DocumentMut::new()
    };
    values["reviewed"] = value(reviewed);
    if document.living()
        && let Some(fingerprint) = fingerprint
    {
        values["sources_hash"] = value(fingerprint);
    }
    let line_end = if document.source.text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let content = values
        .to_string()
        .replace("\r\n", "\n")
        .replace('\n', line_end);
    let bom = if document.source.text.starts_with('\u{feff}') {
        "\u{feff}"
    } else {
        ""
    };
    let body = &document.source.text[document.metadata.body_offset..];
    Ok(format!("{bom}+++{line_end}{content}+++{line_end}{body}"))
}
