use super::rules::{RuleOverride, RuleSetting, matches};
use crate::shared::models::Outcome;
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Default, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Options {
    #[serde(default)]
    pub plural_forms: BTreeMap<String, String>,
    #[serde(default)]
    pub pairs: BTreeMap<String, Value>,
    #[serde(default)]
    pub external_types: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    #[serde(rename = "match")]
    pub pattern: String,
    pub require: Vec<String>,
}

pub fn options(
    rules: &BTreeMap<String, RuleSetting>,
    overrides: &[RuleOverride],
    id: &str,
    path: &str,
) -> Options {
    if id == "architecture/companion-files"
        && super::rules::level(rules, overrides, id, path, "error") == "off"
    {
        return Options::default();
    }
    let mut merged = Map::new();
    for setting in rules.get(id).into_iter().chain(
        overrides
            .iter()
            .filter(|entry| entry.includes.iter().any(|pattern| matches(pattern, path)))
            .filter_map(|entry| entry.rules.get(id)),
    ) {
        let RuleSetting::Detailed(value) = setting else {
            continue;
        };
        merge_options(&mut merged, &value.options);
    }
    serde_json::from_value(Value::Object(merged)).unwrap_or_default()
}

fn merge_options(merged: &mut Map<String, Value>, options: &BTreeMap<String, Value>) {
    for (key, value) in options {
        if matches!(key.as_str(), "pairs" | "pluralForms") {
            let Some(object) = value.as_object() else {
                continue;
            };
            let target = merged
                .entry(key.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Some(target) = target.as_object_mut() {
                target.extend(object.clone());
            }
        } else {
            merged.insert(key.clone(), value.clone());
        }
    }
}

pub fn validity(id: &str, values: &BTreeMap<String, Value>) -> Outcome<bool> {
    let allowed: &[&str] = match id {
        "architecture/mapper-return-type" => &["pluralForms"],
        "architecture/companion-files" => &["pairs"],
        "architecture/mapper-output-declaration" => &["externalTypes"],
        _ => return Ok(false),
    };
    if values.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("Unsupported options for {id}").into());
    }
    let options: Options = serde_json::from_value(serde_json::to_value(values)?)?;
    for (name, plural) in &options.plural_forms {
        if !identifier(name) || !identifier(plural) {
            return Err("Invalid mapper pluralForms".into());
        }
    }
    if options
        .external_types
        .iter()
        .any(|name| name.is_empty() || name.contains(['*', '?']))
    {
        return Err("External types must be exact names".into());
    }
    for (name, value) in options.pairs {
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err("Invalid companion name".into());
        }
        if value == false {
            continue;
        }
        let pair: Pair = serde_json::from_value(value)?;
        if pair.pattern.is_empty()
            || pair.pattern.contains(['\\', ':'])
            || pair
                .pattern
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
            || pair.require.is_empty()
        {
            return Err("Invalid companion pattern".into());
        }
        let captures = capture_names(&pair.pattern)?;
        for template in pair.require {
            if template.is_empty()
                || template.starts_with('/')
                || template.contains(['\\', ':', '*', '?'])
                || template.split('/').any(str::is_empty)
            {
                return Err("Invalid companion template".into());
            }
            let mut text = template;
            for name in &captures {
                text = text.replace(&format!("${{{name}}}"), "capture");
            }
            if text.contains(['{', '}', '$']) {
                return Err("Unknown companion capture".into());
            }
        }
    }
    Ok(true)
}

fn identifier(value: &str) -> bool {
    value.starts_with(|c: char| c.is_ascii_alphabetic())
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn capture_names(pattern: &str) -> Outcome<Vec<String>> {
    let mut names = Vec::new();
    let mut rest = pattern;
    while let Some((prefix, tail)) = rest.split_once('{') {
        if prefix.contains('}') {
            return Err("Invalid companion capture".into());
        }
        let (name, tail) = tail.split_once('}').ok_or("Invalid companion capture")?;
        if !identifier(name) || names.iter().any(|other| other == name) {
            return Err("Invalid companion capture".into());
        }
        names.push(name.to_owned());
        rest = tail;
    }
    if rest.contains('}') {
        return Err("Invalid companion capture".into());
    }
    Ok(names)
}

pub fn plural(name: &str, options: &Options) -> String {
    if let Some(value) = options.plural_forms.get(name) {
        return value.clone();
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with('y')
        && lower.len() > 1
        && !"aeiou".contains(char::from(lower.as_bytes()[lower.len() - 2]))
    {
        return format!("{}ies", &name[..name.len() - 1]);
    }
    if ["s", "x", "z", "ch", "sh"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
    {
        return format!("{name}es");
    }
    format!("{name}s")
}

pub fn snake(name: &str) -> String {
    let chars: Vec<_> = name.chars().collect();
    let mut output = String::new();
    for (index, c) in chars.iter().enumerate() {
        if index > 0
            && c.is_ascii_uppercase()
            && (chars[index - 1].is_ascii_lowercase()
                || chars[index - 1].is_ascii_digit()
                || chars.get(index + 1).is_some_and(char::is_ascii_lowercase))
        {
            output.push('_');
        }
        output.push(c.to_ascii_lowercase());
    }
    output
}

pub fn output_name(name: &str, array: bool, options: &Options) -> String {
    if !array {
        return snake(name);
    }
    if let Some(plural) = options.plural_forms.get(name) {
        return snake(plural);
    }
    plural(&snake(name), &Options::default())
}

pub fn matches_output(function: &str, name: &str, array: bool, options: &Options) -> bool {
    let expected = output_name(name, array, options);
    function
        .strip_prefix("map_")
        .is_some_and(|suffix| suffix == expected || suffix.ends_with(&format!("_to_{expected}")))
}

pub fn captures(pattern: &str, path: &str) -> Option<BTreeMap<String, String>> {
    visit(
        &pattern.chars().collect::<Vec<_>>(),
        &path.chars().collect::<Vec<_>>(),
        BTreeMap::new(),
    )
}

fn visit(
    pattern: &[char],
    path: &[char],
    values: BTreeMap<String, String>,
) -> Option<BTreeMap<String, String>> {
    let Some(first) = pattern.first() else {
        return path.is_empty().then_some(values);
    };
    if *first == '{' {
        let end = pattern.iter().position(|c| *c == '}')?;
        let name: String = pattern[1..end].iter().collect();
        for count in 1..=path.iter().position(|c| *c == '/').unwrap_or(path.len()) {
            let mut next = values.clone();
            next.insert(name.clone(), path[..count].iter().collect());
            if let Some(found) = visit(&pattern[end + 1..], &path[count..], next) {
                return Some(found);
            }
        }
        return None;
    }
    if *first == '*' {
        let double = pattern.get(1) == Some(&'*');
        let rest = &pattern[if double { 2 } else { 1 }..];
        if double
            && rest.first() == Some(&'/')
            && let Some(found) = visit(&rest[1..], path, values.clone())
        {
            return Some(found);
        }
        let limit = if double {
            path.len()
        } else {
            path.iter().position(|c| *c == '/').unwrap_or(path.len())
        };
        for count in 0..=limit {
            if let Some(found) = visit(rest, &path[count..], values.clone()) {
                return Some(found);
            }
        }
        return None;
    }
    if path
        .first()
        .is_some_and(|c| c == first || (*first == '?' && *c != '/'))
    {
        return visit(&pattern[1..], &path[1..], values);
    }
    None
}

pub fn missing_companions(
    root: &Path,
    file: &str,
    options: &Options,
) -> Outcome<Vec<(String, String)>> {
    let mut missing = Vec::new();
    for (id, value) in &options.pairs {
        if value == false {
            continue;
        }
        let pair: Pair = serde_json::from_value(value.clone())?;
        let Some(values) = captures(&pair.pattern, file) else {
            continue;
        };
        for template in pair.require {
            let mut template = template;
            for (name, value) in &values {
                template = template.replace(&format!("${{{name}}}"), value);
            }
            let joined = Path::new(file)
                .parent()
                .unwrap_or(Path::new(""))
                .join(template);
            let target = normalize_target(&joined, id)?;
            if !exact_file(root, &target)? {
                missing.push((id.clone(), target.to_string_lossy().replace('\\', "/")));
            }
        }
    }
    Ok(missing)
}

fn normalize_target(joined: &Path, id: &str) -> Outcome<PathBuf> {
    let mut target = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::Normal(value) => target.push(value),
            Component::CurDir => {}
            Component::ParentDir if target.pop() => {}
            _ => return Err(format!("Companion {id} escapes project").into()),
        }
    }
    Ok(target)
}

fn exact_file(root: &Path, relative: &Path) -> Outcome<bool> {
    let mut path = root.to_path_buf();
    for component in relative.components() {
        if !path.is_dir()
            || !std::fs::read_dir(&path)?
                .filter_map(Result::ok)
                .any(|entry| entry.file_name() == component.as_os_str())
        {
            return Ok(false);
        }
        path.push(component);
    }
    Ok(path.is_file() && path.canonicalize()?.starts_with(root.canonicalize()?))
}
