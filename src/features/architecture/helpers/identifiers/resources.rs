use super::{NAME, Scripts, TYPE};
use crate::shared::{
    helpers::issue,
    models::{Corpus, Diagnostic, Source},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn check_declaration(
    source: &Source,
    scripts: &Scripts<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !scripts
        .ancestry(&source.path)
        .iter()
        .any(|base| base == "Resource")
    {
        return;
    }
    let Some(field) = scripts
        .scripts
        .get(&source.path)
        .and_then(|script| script.fields.get("id"))
    else {
        return;
    };
    if field.exported && field.ty != "StringName" && !matches!(field.ty.as_str(), "int" | "float") {
        diagnostics.push(issue(
            &source.path,
            field.line,
            field.column,
            TYPE,
            "id",
            &field.ty,
            "Exported primary resource id: StringName",
        ));
    }
}

#[derive(Default)]
struct Section {
    class: String,
    line: usize,
    main: bool,
    properties: Vec<(String, String, usize, usize)>,
}

pub(super) fn check(corpus: &Corpus, scripts: &Scripts<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let mut identities: BTreeMap<String, Vec<(&Source, usize)>> = BTreeMap::new();
    for source in &corpus.sources {
        if !source.path.ends_with(".tres") && !source.path.ends_with(".tscn") {
            continue;
        }
        for section in sections(source, scripts) {
            if let Some((id, line)) = check_section(source, &section, scripts, diagnostics) {
                identities.entry(id).or_default().push((source, line));
            }
        }
    }
    for (id, entries) in identities.iter().filter(|(_, entries)| entries.len() > 1) {
        for (source, line) in entries {
            diagnostics.push(issue(
                &source.path,
                *line,
                1,
                NAME,
                "id",
                id,
                "Globally unique standalone resource ID; choose a distinct filename subject",
            ));
        }
    }
}

fn check_section(
    source: &Source,
    section: &Section,
    scripts: &Scripts<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(String, usize)> {
    for (name, value, line, column) in &section.properties {
        if let Some(property) = scripts.field(&section.class, name) {
            diagnostics.extend(super::literals::resource_literals(
                source,
                scripts,
                &property.ty,
                value,
                *line,
                *column,
            ));
        }
    }
    let field = scripts
        .field(&section.class, "id")
        .filter(|field| field.exported)?;
    if !section.main
        || !source.path.ends_with(".tres")
        || matches!(field.ty.as_str(), "int" | "float")
    {
        return None;
    }
    let explicit = section.properties.iter().find(|(name, ..)| name == "id");
    let mut value = explicit.map_or(field.value.as_str(), |(_, value, ..)| value.as_str());
    let mut seen = BTreeSet::new();
    while literal(value).is_none() && seen.insert(value) {
        let Some(constant) = scripts.field(&section.class, value) else {
            break;
        };
        value = &constant.value;
    }
    let line = explicit.map_or(section.line, |(_, _, line, _)| *line);
    let stem = source
        .path
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim_end_matches(".tres");
    let expected = stem.strip_suffix("_resource").unwrap_or(stem);
    let actual = literal(value);
    if actual.as_deref() != Some(expected) {
        diagnostics.push(issue(&source.path, line, 1, NAME, "id", value, expected));
    }
    actual.filter(|id| !id.is_empty()).map(|id| (id, line))
}

fn sections(source: &Source, scripts: &Scripts<'_>) -> Vec<Section> {
    let mut external = BTreeMap::new();
    let mut sections = Vec::new();
    let mut current: Option<Section> = None;
    for (index, line) in source.text.lines().enumerate() {
        if let Some(section) = &mut current
            && let Some((_, value, _, _)) = section.properties.last_mut()
            && unclosed(value)
        {
            value.push('\n');
            value.push_str(line);
            continue;
        }
        let leading = line.len() - line.trim_start().len();
        let line = line.trim();
        if line.starts_with(';') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            if let Some(section) = current.take() {
                sections.push(section);
            }
            if line.starts_with("[ext_resource ")
                && let (Some(id), Some(path)) = (attribute(line, "id"), attribute(line, "path"))
            {
                external.insert(id, path.trim_start_matches("res://").to_owned());
            }
            if line == "[resource]" || line.starts_with("[sub_resource ") {
                current = Some(Section {
                    line: index + 1,
                    main: line == "[resource]",
                    ..Section::default()
                });
            }
            continue;
        }
        let Some(section) = &mut current else {
            continue;
        };
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        let column =
            leading + line.find('=').unwrap_or(0) + 2 + value.len() - value.trim_start().len();
        let value = value.trim();
        if name == "script"
            && let Some(handle) = value
                .strip_prefix("ExtResource(")
                .and_then(|tail| tail.strip_suffix(')'))
                .and_then(literal)
            && let Some(path) = external.get(&handle)
        {
            section.class = scripts.path(path).into();
        }
        section
            .properties
            .push((name.into(), value.into(), index + 1, column));
    }
    if let Some(section) = current {
        sections.push(section);
    }
    sections
}

fn attribute(line: &str, name: &str) -> Option<String> {
    let tail = line.split_once(&format!("{name}=\""))?.1;
    literal(&format!("\"{tail}"))
}

pub(super) fn literal(value: &str) -> Option<String> {
    let value = value.trim().strip_prefix('&').unwrap_or(value.trim());
    if !value.starts_with('"') {
        return None;
    }
    let mut end = 1;
    let mut escaped = false;
    for ch in value[1..].chars() {
        end += ch.len_utf8();
        if ch == '"' && !escaped {
            return serde_json::from_str(&value[..end]).ok();
        }
        escaped = ch == '\\' && !escaped;
    }
    None
}

fn unclosed(value: &str) -> bool {
    let mut quoted = false;
    let mut escaped = false;
    let mut depth = 0_i32;
    for ch in value.chars() {
        if ch == '"' && !escaped {
            quoted = !quoted;
        }
        if !quoted {
            if "([{ ".contains(ch) && ch != ' ' {
                depth += 1;
            }
            if ")] }".contains(ch) && ch != ' ' {
                depth -= 1;
            }
        }
        escaped = ch == '\\' && !escaped;
    }
    quoted || depth > 0
}
