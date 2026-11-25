use super::FileRole;
use crate::shared::models::{Corpus, Source};

pub fn destination(source: &Source, role: &FileRole, corpus: &Corpus, bundled: bool) -> String {
    let local = source
        .path
        .strip_prefix(&format!("{}/", source.owner))
        .unwrap_or(&source.path);
    let filename = local.rsplit('/').next().unwrap_or(local);
    let (stem, extension) = filename.rsplit_once('.').unwrap_or((filename, ""));
    let suffix = format!("_{}", role.role);
    let mut subject = normalize(stem.strip_suffix(&suffix).unwrap_or(stem));
    if role.role == "music" {
        subject = subject
            .strip_prefix("music_")
            .unwrap_or(&subject)
            .to_owned();
    }
    if role.role == "resource" {
        subject = resource_subject(source, &subject, corpus);
    }
    if !subject.chars().any(|ch| ch.is_ascii_lowercase()) {
        subject = format!("custom_{subject}").trim_end_matches('_').to_owned();
    }
    let name = format!("{subject}_{role_name}.{extension}", role_name = role.role);
    if bundled {
        return format!("{}/{name}", super::super::bundles::parent(&source.path));
    }
    let mut directories = local
        .rsplit_once('/')
        .map_or("", |(directory, _)| directory);
    if let Some(tail) = directories
        .strip_prefix(&role.folder)
        .filter(|tail| tail.is_empty() || tail.starts_with('/'))
    {
        directories = tail.trim_start_matches('/');
    } else if let Some(alias) = role.aliases.iter().find(|alias| {
        directories == alias.as_str() || directories.starts_with(&format!("{alias}/"))
    }) {
        directories = directories
            .strip_prefix(alias)
            .unwrap_or("")
            .trim_start_matches('/');
    } else if let Some(tail) = directories.strip_prefix("assets/") {
        directories = tail;
    } else if role.folder.starts_with("assets/") {
        directories = "";
    }
    [
        source.owner.as_str(),
        role.folder.as_str(),
        directories,
        &name,
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("/")
}

fn resource_subject(source: &Source, subject: &str, corpus: &Corpus) -> String {
    let kind = resource_type(source, corpus).map(normalize).map(|name| {
        name.trim_end_matches("_resource")
            .trim_end_matches("_definition")
            .to_owned()
    });
    let mut subject = subject.to_owned();
    if !subject.chars().any(|ch| ch.is_ascii_lowercase()) {
        let parent = source.path.rsplit('/').nth(1).unwrap_or("custom");
        subject = format!("{}_{}", normalize(parent), subject);
    }
    if let Some(kind) = kind.filter(|kind| !kind.is_empty())
        && subject != kind
        && !subject.ends_with(&format!("_{kind}"))
    {
        subject = format!("{subject}_{kind}");
    }
    subject
}

fn attribute<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.split_once(&format!("{key}=\""))?.1.split('"').next()
}

fn resource_type<'a>(source: &'a Source, corpus: &'a Corpus) -> Option<&'a str> {
    let header = source
        .text
        .lines()
        .find(|line| line.starts_with("[gd_resource"))?;
    if let Some(class) = attribute(header, "script_class") {
        return Some(class);
    }
    // Resolve the attached script rather than unrelated external resources.
    let script_id = source.text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("script = ExtResource(\"")?
            .split('"')
            .next()
    });
    if let Some(id) = script_id {
        let script_line = source.text.lines().find(|line| {
            line.starts_with("[ext_resource")
                && attribute(line, "type") == Some("Script")
                && attribute(line, "id") == Some(id)
        });
        if let Some(path) = script_line.and_then(|line| attribute(line, "path")) {
            let target = path
                .strip_prefix("res://")
                .or_else(|| corpus.uids.get(path).map(String::as_str));
            if let Some(class) = corpus
                .sources
                .iter()
                .chain(&corpus.excluded_sources)
                .find(|script| Some(script.path.as_str()) == target)
                .and_then(|script| script.class_name.as_deref())
            {
                return Some(class);
            }
        }
    }
    attribute(header, "type").filter(|kind| *kind != "Resource")
}

fn normalize(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        if ch.is_ascii_uppercase() && !output.is_empty() && !output.ends_with('_') {
            output.push('_');
        }
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
        } else if !output.is_empty() && !output.ends_with('_') {
            output.push('_');
        }
    }
    output.trim_matches('_').to_owned()
}
