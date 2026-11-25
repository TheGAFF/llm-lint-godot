use crate::{
    features::configuration::{models::Project, rules},
    shared::{
        helpers::issue,
        models::{Corpus, Diagnostic, Outcome},
    },
};

pub fn check(project: &Project, corpus: &Corpus, diagnostics: &mut Vec<Diagnostic>) -> Outcome<()> {
    let catalog = rules::catalog()?;
    for source in &corpus.sources {
        for (index, body) in comments(source)? {
            let body = body.as_str();
            if !body.starts_with("llm-lint-ignore") {
                continue;
            }
            let directive = body
                .strip_prefix("llm-lint-ignore ")
                .and_then(|tail| tail.split_once(':'));
            let valid = directive.and_then(|(rule, reason)| {
                let definition = catalog.iter().find(|entry| entry["id"] == rule)?;
                (!reason.trim().is_empty()
                    && !rule.contains([' ', ',', '*'])
                    && rule.starts_with("architecture/")
                    && (definition["configurable"] != false
                        || [super::bundles::PLACEMENT, super::bundles::COMPANION].contains(&rule)))
                .then_some((rule, definition))
            });
            let Some((rule, definition)) = valid else {
                diagnostics.push(issue(
                    &source.path,
                    index + 1,
                    1,
                    "architecture/comment-suppression",
                    "<comment>",
                    body,
                    "llm-lint-ignore <one exact suppressible architecture rule>: <nonempty reason>",
                ));
                continue;
            };
            let default = definition["default"].as_str().unwrap_or("error");
            let enabled = rules::level(
                &project.settings.rules,
                &project.settings.overrides,
                rule,
                &source.path,
                default,
            ) != "off";
            let count = diagnostics.len();
            remove(diagnostics, &source.path, rule, index + 2, enabled);
            if count == diagnostics.len() {
                diagnostics.push(issue(
                    &source.path,
                    index + 1,
                    1,
                    "architecture/comment-suppression",
                    "<comment>",
                    body,
                    "Remove unused next-line suppression",
                ));
            }
        }
    }
    Ok(())
}

fn remove(diagnostics: &mut Vec<Diagnostic>, path: &str, rule: &str, line: usize, enabled: bool) {
    if enabled {
        diagnostics.retain(|d| !(d.path == path && d.rule == rule && d.line == line));
    }
}

fn comments(source: &crate::shared::models::Source) -> Outcome<Vec<(usize, String)>> {
    if source.path.ends_with(".gd") {
        return super::identifiers::comments(source);
    }
    if !source.path.ends_with(".tres") && !source.path.ends_with(".tscn") {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    let mut quoted = false;
    let mut escaped = false;
    for (index, line) in source.text.lines().enumerate() {
        if !quoted && let Some(body) = line.trim().strip_prefix(';') {
            result.push((index, body.trim().into()));
            continue;
        }
        for ch in line.chars() {
            if ch == ';' && !quoted {
                break;
            }
            if ch == '"' && !escaped {
                quoted = !quoted;
            }
            escaped = ch == '\\' && !escaped;
        }
        escaped = false;
    }
    Ok(result)
}
