use super::super::predicates::{is_asset, is_classified};
use crate::{
    features::configuration::models::Project,
    shared::{
        helpers::{claimed_role, issue, pascal_case, registry, snake_case},
        models::{Diagnostic, Source},
    },
};
use std::path::Path;

pub const STRUCTURAL: &[(&str, &str)] = &[
    ("nodes", "node"),
    ("resources", "resource"),
    ("models", "model"),
    ("services", "service"),
    ("clients", "client"),
    ("errors", "error"),
    ("constants", "constants"),
];

pub fn check_roles(
    project: &Project,
    source: &Source,
    corpus: &crate::shared::models::Corpus,
    diagnostics: &mut Vec<Diagnostic>,
) -> crate::shared::models::Outcome<()> {
    check_suppressions(source, diagnostics);
    let entrypoint = project
        .settings
        .entrypoints
        .iter()
        .any(|entry| entry.path == source.path);
    if source.layer == "unregistered" && !entrypoint {
        diagnostics.push(issue(
            &source.path,
            1,
            1,
            "architecture/feature-layout",
            "<script>",
            "Authored source has no feature owner",
            "features/<owner>, shared, app or a registered plugin/tool owner",
        ));
    }
    let local = source
        .path
        .strip_prefix(&format!("{}/", source.owner))
        .unwrap_or(&source.path);
    let stem = Path::new(&source.path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("");
    let mut components = source.path.split('/').peekable();
    while let Some(file) = components.next() {
        if components.peek().is_none() && super::bundles::native_locale_filename(file) {
            continue;
        }
        let part = file.split('.').next().unwrap_or("");
        if !snake_case(part) {
            diagnostics.push(issue(
                &source.path,
                1,
                1,
                "architecture/file-name",
                "<script>",
                part,
                "snake_case paths",
            ));
        }
    }
    if !source.path.ends_with(".gd") {
        if (is_asset(&source.path, &super::file_roles::catalog()?)
            || is_classified(project, &source.path))
            && !entrypoint
        {
            super::file_roles::check(project, source, corpus, diagnostics)?;
        }
        return Ok(());
    }
    if let Some(name) = &source.class_name {
        let expected = pascal_case(stem);
        let matches = name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
            && name.chars().all(|ch| ch.is_ascii_alphanumeric())
            && name.to_ascii_lowercase() == stem.replace('_', "");
        if !matches {
            diagnostics.push(issue(
                &source.path,
                1,
                1,
                "architecture/class-name",
                name,
                name,
                &expected,
            ));
        }
    }
    if source.testing {
        check_testing(source, stem, local, diagnostics);
        return Ok(());
    }
    if entrypoint {
        return Ok(());
    }
    check_layout(project, source, corpus, stem, local, diagnostics)
}

fn check_suppressions(source: &Source, diagnostics: &mut Vec<Diagnostic>) {
    let lines: Vec<_> = source.text.lines().collect();
    for (line_number, line) in &source.native_suppressions {
        let precise = line
            .split_once("gdstyle:ignore=")
            .and_then(|(_, rest)| {
                rest.split_once(" # reason:").or_else(|| {
                    let index = line_number.saturating_sub(1);
                    [index.checked_sub(1), index.checked_add(1)]
                        .into_iter()
                        .flatten()
                        .filter_map(|adjacent| lines.get(adjacent))
                        .filter_map(|adjacent| adjacent.trim().strip_prefix("# reason:"))
                        .find(|reason| !reason.trim().is_empty())
                        .map(|reason| (rest, reason))
                })
            })
            .is_some_and(|(rule, reason)| {
                !rule.trim().is_empty()
                    && rule.contains('/')
                    && !rule.contains([',', '*', ' '])
                    && !reason.trim().is_empty()
            });
        if !precise {
            diagnostics.push(issue(
                &source.path,
                *line_number,
                1,
                "architecture/native-suppression",
                "<script>",
                line.trim(),
                "One exact gdstyle rule with a same-line or adjacent '# reason: <explanation>'; no file-wide suppression",
            ));
        }
    }
}

fn check_layout(
    project: &Project,
    source: &Source,
    corpus: &crate::shared::models::Corpus,
    stem: &str,
    local: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> crate::shared::models::Outcome<()> {
    let role = local.split('/').next().unwrap_or("");
    let roles = registry()?;
    let functional = roles
        .iter()
        .map(|role| (role.1.as_str(), role.0.as_str()))
        .chain(std::iter::once(("helpers", "helpers")))
        .collect::<Vec<_>>();
    if super::bundles::enabled(project, source) {
        let collection = functional
            .iter()
            .find(|(plural, _)| has_role_suffix(stem, plural));
        let suffix = STRUCTURAL
            .iter()
            .chain(&functional)
            .find(|(_, singular)| has_role_suffix(stem, singular));
        let Some((role, _)) = suffix.or(collection) else {
            diagnostics.push(issue(
                &source.path,
                1,
                1,
                "architecture/role-filename",
                "<script>",
                local,
                "<subject>_<script role>.gd",
            ));
            check_functions(source, None, "<unresolved>", &roles, diagnostics);
            return Ok(());
        };
        check_base(source, corpus, role, diagnostics)?;
        let functional_role = functional
            .iter()
            .any(|(plural, _)| plural == role)
            .then_some(*role);
        if collection.is_some() {
            check_collection_size(project, source, diagnostics);
        }
        check_functions(source, functional_role, role, &roles, diagnostics);
        return Ok(());
    }
    let suffix = STRUCTURAL
        .iter()
        .chain(&functional)
        .find(|(plural, _)| *plural == role);
    let collection = functional
        .iter()
        .find(|(plural, _)| stem.ends_with(&format!("_{plural}")) && !local.contains('/'));
    let functional_role = functional
        .iter()
        .find(|(plural, _)| *plural == role)
        .map(|(plural, _)| *plural)
        .or_else(|| collection.map(|(plural, _)| *plural));
    if let Some((_, singular)) = suffix {
        if !stem.ends_with(&format!("_{singular}")) || local.split('/').count() != 2 {
            diagnostics.push(issue(
                &source.path,
                1,
                1,
                "architecture/role-filename",
                "<script>",
                local,
                &format!("{role}/<subject>_{singular}.gd"),
            ));
        }
        check_base(source, corpus, role, diagnostics)?;
    } else if collection.is_none() {
        diagnostics.push(issue(
            &source.path,
            1,
            1,
            "architecture/role-placement",
            "<script>",
            local,
            "Role folder with matching suffix, or owner-level functional collection",
        ));
    }
    if collection.is_some() {
        check_collection_size(project, source, diagnostics);
    }
    check_functions(source, functional_role, role, &roles, diagnostics);
    Ok(())
}

fn has_role_suffix(stem: &str, role: &str) -> bool {
    stem.strip_suffix(role)
        .and_then(|subject| subject.strip_suffix('_'))
        .is_some_and(|subject| subject.chars().any(|ch| ch.is_ascii_lowercase()))
}

fn check_collection_size(project: &Project, source: &Source, diagnostics: &mut Vec<Diagnostic>) {
    let limit = crate::features::configuration::rules::threshold_for_path(
        &project.settings.rules,
        &project.settings.overrides,
        "architecture/collection-size",
        "max",
        &source.path,
        project.settings.collection_max_code_lines,
    );
    if source.code_lines > limit {
        diagnostics.push(issue(
            &source.path,
            1,
            1,
            "architecture/collection-size",
            "<script>",
            &source.code_lines.to_string(),
            &format!("At most {limit} code lines"),
        ));
    }
}

fn check_functions(
    source: &Source,
    functional_role: Option<&str>,
    role: &str,
    roles: &[crate::shared::models::Role],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for function in source
        .functions
        .iter()
        .filter(|function| function.static_method && !function.name.starts_with('_'))
    {
        let expected = claimed_role(&function.name, roles);
        if functional_role != Some(expected.as_str()) {
            diagnostics.push(issue(
                &source.path,
                function.line,
                function.column,
                "architecture/function-role",
                &function.name,
                functional_role.unwrap_or(role),
                &expected,
            ));
        }
    }
    if functional_role.is_some()
        && (source.instance_state
            || source
                .functions
                .iter()
                .any(|method| !method.static_method && !method.name.starts_with('_')))
    {
        diagnostics.push(issue(
            &source.path,
            1,
            1,
            "architecture/utility-state",
            "<script>",
            "State or public instance methods in a functional utility",
            "Static functions with private implementation helpers",
        ));
    }
}

fn check_base(
    source: &Source,
    corpus: &crate::shared::models::Corpus,
    role: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> crate::shared::models::Outcome<()> {
    let base = ancestry(source, corpus);
    let engine = engine_role_bases()?;
    let base = engine.get(&base).map_or(base.as_str(), String::as_str);
    let expected = match role {
        "resources" if base == "Resource" => return Ok(()),
        "resources" => "Resource inheritance",
        "models" | "services" | "clients" if base == "RefCounted" => return Ok(()),
        "models" | "services" | "clients" => "RefCounted inheritance",
        "nodes" if ["Resource", "RefCounted"].contains(&base) => "Node inheritance",
        _ => return Ok(()),
    };
    diagnostics.push(issue(
        &source.path,
        1,
        1,
        "architecture/role-base",
        "<script>",
        base,
        expected,
    ));
    Ok(())
}

fn engine_role_bases()
-> crate::shared::models::Outcome<&'static std::collections::BTreeMap<String, String>> {
    use std::{collections::BTreeMap, sync::OnceLock};
    static BASES: OnceLock<Result<BTreeMap<String, String>, String>> = OnceLock::new();
    BASES
        .get_or_init(|| {
            #[derive(serde::Deserialize)]
            struct Catalog {
                role_bases: BTreeMap<String, String>,
            }
            serde_json::from_str::<Catalog>(include_str!("../../../../policy/engine-classes.json"))
                .map(|catalog| catalog.role_bases)
                .map_err(|error| format!("Invalid engine ancestry catalog: {error}"))
        })
        .as_ref()
        .map_err(|error| error.clone().into())
}

fn ancestry(source: &Source, corpus: &crate::shared::models::Corpus) -> String {
    let mut base = source.base.as_deref().unwrap_or("RefCounted").to_owned();
    let mut visited = std::collections::BTreeSet::new();
    while visited.insert(base.clone()) {
        let path = base.trim_matches(['\'', '"']).strip_prefix("res://");
        let parent = corpus
            .sources
            .iter()
            .chain(&corpus.excluded_sources)
            .find(|source| {
                source.class_name.as_deref() == Some(base.as_str())
                    || path == Some(source.path.as_str())
            });
        let Some(parent) = parent else {
            break;
        };
        base = parent.base.as_deref().unwrap_or("RefCounted").into();
    }
    base
}

fn check_testing(source: &Source, stem: &str, local: &str, diagnostics: &mut Vec<Diagnostic>) {
    if !is_test_stem(stem) {
        diagnostics.push(issue(
            &source.path,
            1,
            1,
            "architecture/testing-name",
            "<script>",
            local,
            "test_*.gd, *_test_factory.gd or *_test_harness.gd",
        ));
    }
}

pub fn is_test_stem(stem: &str) -> bool {
    stem.starts_with("test_") || stem.ends_with("_test_factory") || stem.ends_with("_test_harness")
}
