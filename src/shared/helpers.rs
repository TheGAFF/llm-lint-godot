use super::models::{Diagnostic, Role};

pub fn registry() -> super::models::Outcome<Vec<Role>> {
    // Build-time parity is checked by the repository's role-catalog generator.
    Ok(serde_json::from_str(include_str!(
        "../../policy/functional-roles.json"
    ))?)
}

pub fn snake_case(name: &str) -> bool {
    !name.is_empty()
        && name.split('_').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        })
}

pub fn pascal_case(name: &str) -> String {
    name.split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or(String::new(), |first| {
                format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
            })
        })
        .collect()
}

pub fn claimed_role(name: &str, roles: &[Role]) -> String {
    let first = name.split('_').next().unwrap_or("");
    roles
        .iter()
        .find(|role| role.2.split_whitespace().any(|verb| verb == first))
        .map_or_else(|| "helpers".into(), |role| role.1.clone())
}

pub fn issue(
    path: &str,
    line: usize,
    column: usize,
    rule: &str,
    item: &str,
    actual: &str,
    expected: &str,
) -> Diagnostic {
    Diagnostic {
        severity: "error".into(),
        path: path.into(),
        line,
        column,
        rule: rule.into(),
        item: item.into(),
        actual: actual.into(),
        expected: expected.into(),
        document_sources: None,
    }
}
