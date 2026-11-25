use super::{FILE_KINDS, Gap};
pub(super) use crate::features::parsing::helpers::resource_paths::{attribute, parent, strings};
use crate::{
    features::parsing::parsers::{io_call, path_arguments},
    shared::models::Outcome,
};
use std::{collections::BTreeSet, fs, path::Path};

pub(super) fn extension(path: &str) -> &str {
    path.rsplit_once('/')
        .map_or(path, |(_, name)| name)
        .rsplit_once('.')
        .map_or("", |(_, ext)| ext)
}

pub(super) fn ancillary(path: &str) -> bool {
    matches!(extension(path), "uid" | "import" | "remap" | "translation")
}

pub(super) fn tool_source(path: &str) -> bool {
    matches!(
        extension(path),
        "py" | "ps1" | "sh" | "bash" | "cmd" | "bat" | "js" | "mjs" | "ts"
    )
}

pub(super) fn settings(text: &str) -> impl Iterator<Item = (usize, &str, &str, &str)> {
    let mut section = "";
    let mut lines = text.split_inclusive('\n').enumerate();
    let mut offset = 0;
    std::iter::from_fn(move || {
        while let Some((index, raw)) = lines.next() {
            let start = offset;
            offset += raw.len();
            let line = raw.trim();
            if let Some(name) = line
                .strip_prefix('[')
                .and_then(|line| line.strip_suffix(']'))
            {
                section = name;
                continue;
            }
            let Some((key, inline)) = raw
                .split_once('=')
                .filter(|_| !line.starts_with(['[', ';', '#']))
            else {
                continue;
            };
            offset += continuation_bytes(inline, &mut lines);
            return Some((
                index + 1,
                section,
                key.trim(),
                text[start + key.len() + 1..offset].trim(),
            ));
        }
        None
    })
}

fn continuation_bytes<'a>(
    first: &str,
    lines: &mut impl Iterator<Item = (usize, &'a str)>,
) -> usize {
    let mut depth = nesting(first);
    let mut bytes = 0;
    while depth > 0 {
        let Some((_, line)) = lines.next() else {
            break;
        };
        bytes += line.len();
        depth += nesting(line);
    }
    bytes
}

fn nesting(line: &str) -> i32 {
    let mut depth = 0;
    let mut quoted = false;
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => quoted = !quoted,
            '\\' if quoted => {
                chars.next();
            }
            _ if quoted => (),
            '#' | ';' => break,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => (),
        }
    }
    depth
}

pub(super) fn json_paths(
    text: &str,
    omit_release_exclusions: bool,
) -> Result<BTreeSet<String>, serde_json::Error> {
    let mut value: serde_json::Value = serde_json::from_str(text)?;
    if omit_release_exclusions && let Some(fields) = value.as_object_mut() {
        fields.remove("asset_exclusions");
    }
    let mut paths = BTreeSet::new();
    collect_json(value, &mut paths);
    Ok(paths)
}

fn collect_json(value: serde_json::Value, paths: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::String(value) => {
            paths.insert(value);
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_json(value, paths);
            }
        }
        serde_json::Value::Object(values) => {
            for (key, value) in values {
                paths.insert(key);
                collect_json(value, paths);
            }
        }
        _ => (),
    }
}

#[derive(Default)]
pub(super) struct ExportPreset<'a> {
    filter: &'a str,
    include: Vec<&'a str>,
    exclude: Vec<&'a str>,
    files: BTreeSet<&'a str>,
    options: BTreeSet<&'a str>,
}

impl ExportPreset<'_> {
    pub(super) fn includes(&self, path: &str) -> bool {
        let explicit = self.options.contains(path)
            || (matches!(self.filter, "scenes" | "resources") && self.files.contains(path));
        (explicit || self.include.iter().any(|pat| export_glob(pat, path)))
            && !self.exclude.iter().any(|pat| export_glob(pat, path))
    }
}

pub(super) fn presets(text: &str) -> Vec<ExportPreset<'_>> {
    let mut presets = std::collections::BTreeMap::new();
    for (_, section, key, value) in settings(text) {
        let Some(id) = section.strip_prefix("preset.") else {
            continue;
        };
        let id = id.split('.').next().unwrap_or(id);
        let preset: &mut ExportPreset<'_> = presets.entry(id).or_default();
        if section.ends_with(".options") {
            preset
                .options
                .extend(strings(value).filter_map(|path| path.strip_prefix("res://")));
            continue;
        }
        match key {
            "export_filter" => preset.filter = strings(value).next().unwrap_or(""),
            "export_files" => preset
                .files
                .extend(strings(value).map(|path| path.trim_start_matches("res://"))),
            "include_filter" => preset.include = export_patterns(value),
            "exclude_filter" => preset.exclude = export_patterns(value),
            _ => (),
        }
    }
    presets.into_values().collect()
}

fn export_patterns(value: &str) -> Vec<&str> {
    strings(value)
        .next()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

fn export_glob(pattern: &str, path: &str) -> bool {
    // Godot's String.matchn lets '*' match path separators.
    let mut pattern = pattern.trim_start_matches("res://").chars().peekable();
    let mut path = path.chars().peekable();
    let mut wildcard = None;
    while let Some(&ch) = path.peek() {
        let part = pattern.peek().copied();
        if part == Some('*') {
            pattern.next();
            wildcard = Some((pattern.clone(), path.clone()));
        } else if part.is_some_and(|part| glob_character(part, ch)) {
            pattern.next();
            path.next();
        } else if let Some((resume_pattern, retry_path)) = &mut wildcard
            && retry_path.next().is_some()
        {
            pattern = resume_pattern.clone();
            path = retry_path.clone();
        } else {
            return false;
        }
    }
    pattern.all(|part| part == '*')
}

fn glob_character(pattern: char, ch: char) -> bool {
    if pattern == '?' {
        ch != '.'
    } else if pattern.is_ascii() && ch.is_ascii() {
        pattern.eq_ignore_ascii_case(&ch)
    } else {
        pattern.to_lowercase().eq(ch.to_lowercase())
    }
}

pub(super) fn read(root: &Path, path: &str) -> Outcome<String> {
    fs::read_to_string(root.join(path)).map_err(|error| format!("{path}: {error}").into())
}

pub(super) fn file_gap(path: &str, line: usize, message: String) -> Gap {
    Gap {
        path: path.into(),
        line,
        message,
        kinds: FILE_KINDS,
    }
}

pub(super) fn script_io(text: &str) -> Outcome<Vec<(usize, String)>> {
    if ![
        "FileAccess.",
        "Image.load_from_file",
        "load_resource_pack",
        "ResourceUID.",
        "change_scene_to_file",
    ]
    .iter()
    .any(|api| text.contains(api))
    {
        return Ok(Vec::new());
    }
    let tree = crate::features::architecture::helpers::script_tree(text)?;
    let mut stack = vec![tree.root_node()];
    let mut accesses = Vec::new();
    while let Some(node) = stack.pop() {
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
        if let Some((_, argument)) = io_call(node, text, false) {
            let targets = path_arguments(argument, node, text);
            let line = node.start_position().row + 1;
            accesses.extend(targets.into_iter().map(|target| (line, target)));
        }
    }
    Ok(accesses)
}
