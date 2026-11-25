mod recommendations;

use crate::{
    features::configuration::{helpers::inside, models::Project, rules::matches},
    shared::{
        helpers::issue,
        models::{Corpus, Diagnostic, Outcome, Source},
    },
};

#[derive(serde::Deserialize)]
pub struct FileRole {
    pub role: String,
    pub folder: String,
    pub extensions: Vec<String>,
    pub aliases: Vec<String>,
}

pub fn catalog() -> Outcome<Vec<FileRole>> {
    Ok(serde_json::from_str(include_str!(
        "../../../../policy/file-roles.json"
    ))?)
}

pub fn validity(
    project_roles: &[crate::features::configuration::models::AssetClassification],
) -> Outcome<()> {
    let roles = catalog()?;
    for entry in project_roles {
        if !roles
            .iter()
            .any(|role| role.role == entry.role && !role.extensions.iter().any(|ext| ext == "md"))
            || entry.includes.is_empty()
            || entry.includes.iter().any(|path| {
                path.is_empty()
                    || path.starts_with('/')
                    || path.contains(['\\', ':', '[', ']', '{', '}'])
                    || path
                        .split('/')
                        .any(|part| part.is_empty() || part == "." || part == "..")
            })
        {
            return Err(
                "Asset classifications require a known file role and project-relative globs".into(),
            );
        }
    }
    Ok(())
}

pub fn check(
    project: &Project,
    source: &Source,
    corpus: &Corpus,
    diagnostics: &mut Vec<Diagnostic>,
) -> Outcome<()> {
    let roles = catalog()?;
    let extension = source
        .path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let local = source
        .path
        .strip_prefix(&format!("{}/", source.owner))
        .unwrap_or(&source.path);
    let candidates: Vec<_> = roles
        .iter()
        .filter(|role| role.extensions.contains(&extension))
        .collect();
    let chosen = select_role(project, source, local, &candidates);
    let Some(role) = chosen else {
        let choices = candidates
            .iter()
            .map(|role| format!("{} ({})", role.role, role.folder))
            .collect::<Vec<_>>()
            .join(" or ");
        diagnostics.push(issue(
            &source.path,
            1,
            1,
            "architecture/asset-kind",
            "<asset>",
            &source.path,
            &format!(
                "Classify this asset explicitly: {}",
                if choices.is_empty() {
                    "unsupported file type; use a reasoned exclusion for non-runtime inputs"
                } else {
                    &choices
                }
            ),
        ));
        return Ok(());
    };
    let bundled = super::bundles::enabled(project, source);
    let destination = recommendations::destination(source, role, corpus, bundled);
    if !bundled && !inside(local, &role.folder) {
        diagnostics.push(issue(
            &source.path,
            1,
            1,
            "architecture/role-placement",
            "<asset>",
            &source.path,
            &destination,
        ));
    }
    let filename = source.path.rsplit('/').next().unwrap_or("");
    let stem = filename.rsplit_once('.').map_or(filename, |(stem, _)| stem);
    let suffix = format!("_{}", role.role);
    let valid = stem.strip_suffix(&suffix).is_some_and(|subject| {
        crate::shared::helpers::snake_case(subject)
            && subject.chars().any(|ch| ch.is_ascii_lowercase())
    });
    if !valid {
        diagnostics.push(issue(
            &source.path,
            1,
            1,
            "architecture/role-filename",
            "<asset>",
            filename,
            &destination,
        ));
    }
    Ok(())
}

pub fn check_collisions(corpus: &Corpus, diagnostics: &mut Vec<Diagnostic>) {
    let mut destinations: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for diagnostic in diagnostics.iter().filter(|item| {
        item.item == "<asset>"
            && ["architecture/role-filename", "architecture/role-placement"]
                .contains(&item.rule.as_str())
    }) {
        destinations
            .entry(diagnostic.expected.clone())
            .or_default()
            .insert(diagnostic.path.clone());
    }
    for (destination, sources) in destinations {
        for source in &sources {
            if destination != *source && (sources.len() > 1 || corpus.paths.contains(&destination))
            {
                diagnostics.push(issue(source, 1, 1, "architecture/asset-collision", "<asset>", &destination,
                    "Suggested destination exists or is shared by multiple recommendations; choose a unique subject without overwriting it"));
            }
        }
    }
}

fn select_role<'a>(
    project: &Project,
    source: &Source,
    local: &str,
    candidates: &[&'a FileRole],
) -> Option<&'a FileRole> {
    let mut explicit: Vec<_> = project
        .settings
        .asset_classifications
        .iter()
        .filter(|entry| {
            entry
                .includes
                .iter()
                .any(|pattern| matches(pattern, &source.path))
        })
        .map(|entry| entry.role.as_str())
        .collect();
    explicit.sort_unstable();
    explicit.dedup();
    if !explicit.is_empty() {
        return if explicit.len() == 1 {
            candidates
                .iter()
                .copied()
                .find(|role| role.role == explicit[0])
        } else {
            None
        };
    }
    if super::bundles::enabled(project, source) {
        let filename = local.rsplit('/').next().unwrap_or(local);
        let stem = filename.rsplit_once('.').map_or(filename, |(stem, _)| stem);
        if let Some(role) = candidates.iter().copied().find(|role| {
            stem.strip_suffix(&role.role)
                .is_some_and(|prefix| prefix.ends_with('_'))
        }) {
            return Some(role);
        }
    }
    let inferred: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|role| {
            inside(local, &role.folder) || role.aliases.iter().any(|folder| inside(local, folder))
        })
        .collect();
    if inferred.len() == 1 {
        inferred.first().copied()
    } else if candidates.len() == 1 {
        candidates.first().copied()
    } else {
        None
    }
}
