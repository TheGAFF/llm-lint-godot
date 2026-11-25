use crate::shared::models::{Reference, ReferenceOrigin, Source};

pub fn parse_text(path: &str, text: String) -> Source {
    let mut references = Vec::new();
    let mut section = String::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            section = trimmed.to_owned();
        }
        if trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }
        let mut offset = 0;
        while let Some(start) = line[offset..].find('"') {
            let start = offset + start + 1;
            let Some(end) = quoted_end(line, start) else {
                break;
            };
            let value = &line[start..end];
            let item = if section == "[autoload]" {
                format!(
                    "autoload:{}",
                    trimmed.split('=').next().unwrap_or("").trim()
                )
            } else {
                section.clone()
            };
            if value.starts_with("res://") || value.starts_with("uid://") {
                references.push(Reference {
                    origin: ReferenceOrigin::Serialized,
                    target: value.trim_start_matches('*').into(),
                    item: item.clone(),
                    line: index + 1,
                    column: line[..start].chars().count() + 1,
                    dynamic: false,
                    directory: value.ends_with('/'),
                    required: true,
                });
            } else if value.starts_with("*res://") || value.starts_with("*uid://") {
                references.push(Reference {
                    origin: ReferenceOrigin::Serialized,
                    target: value.trim_start_matches('*').into(),
                    item: item.clone(),
                    line: index + 1,
                    column: start + 1,
                    dynamic: false,
                    directory: false,
                    required: true,
                });
            } else if path.ends_with("plugin.cfg") && trimmed.starts_with("script=") {
                let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
                references.push(Reference {
                    origin: ReferenceOrigin::Serialized,
                    target: format!("res://{parent}/{value}"),
                    item: "plugin".into(),
                    line: index + 1,
                    column: start + 1,
                    dynamic: false,
                    directory: false,
                    required: true,
                });
            }
            offset = end + 1;
        }
    }
    Source {
        path: path.into(),
        text,
        owner: String::new(),
        layer: String::new(),
        testing: path.split('/').any(|part| part == "testing"),
        class_name: None,
        base: None,
        functions: vec![],
        references,
        class_uses: vec![],
        diagnostics: vec![],
        code_lines: 0,
        instance_state: false,
        native_suppressions: vec![],
    }
}

fn quoted_end(line: &str, start: usize) -> Option<usize> {
    let mut escaped = false;
    for (offset, ch) in line[start..].char_indices() {
        if ch == '"' && !escaped {
            return Some(start + offset);
        }
        escaped = ch == '\\' && !escaped;
    }
    None
}
