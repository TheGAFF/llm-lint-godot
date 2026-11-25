use super::{Context, Document, markdown::Declaration, metadata};
use crate::{
    features::configuration::{helpers::inside, rules::matches},
    shared::models::{Diagnostic, Outcome, SourceChanges},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

#[derive(Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
struct DeclaredSource {
    path: String,
    purpose: String,
}

pub(super) struct SourceScope {
    declarations: BTreeSet<DeclaredSource>,
    paths: BTreeSet<String>,
    pub(super) missing_lines: Vec<usize>,
}

impl SourceScope {
    pub(super) fn snapshot(
        self,
        mut hash: impl FnMut(&str) -> Outcome<String>,
    ) -> Outcome<Snapshot> {
        let mut files = BTreeMap::new();
        for path in self.paths {
            let content = hash(&path)?;
            files.insert(path, content);
        }
        Ok(Snapshot {
            version: 1,
            declarations: self.declarations,
            files,
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    version: u8,
    declarations: BTreeSet<DeclaredSource>,
    files: BTreeMap<String, String>,
}

impl Snapshot {
    pub(super) fn fingerprint(&self) -> String {
        let mut digest = Sha256::new();
        field(&mut digest, b"godot-llm-lint/document-sources/v1");
        for entry in &self.declarations {
            field(&mut digest, entry.path.as_bytes());
            field(&mut digest, entry.purpose.as_bytes());
        }
        for (path, hash) in &self.files {
            field(&mut digest, path.as_bytes());
            field(&mut digest, hash.as_bytes());
        }
        format!("sha256:{:x}", digest.finalize())
    }

    pub(super) fn verifies(&self, recorded: &str) -> bool {
        self.version == 1
            && self
                .files
                .values()
                .all(|hash| metadata::valid_hash(&format!("sha256:{hash}")))
            && self.fingerprint() == recorded
    }

    pub(super) fn changes_since(&self, reviewed: &Self) -> SourceChanges {
        SourceChanges {
            modified: self
                .files
                .iter()
                .filter(|(path, hash)| {
                    reviewed
                        .files
                        .get(*path)
                        .is_some_and(|previous| previous != *hash)
                })
                .map(|(path, _)| path.clone())
                .collect(),
            added: self
                .files
                .keys()
                .filter(|path| !reviewed.files.contains_key(*path))
                .cloned()
                .collect(),
            removed: reviewed
                .files
                .keys()
                .filter(|path| !self.files.contains_key(*path))
                .cloned()
                .collect(),
            declarations_changed: self.declarations != reviewed.declarations,
        }
    }
}

pub(super) fn safe(pattern: &str) -> bool {
    !pattern.is_empty()
        && pattern.trim() == pattern
        && !pattern.starts_with('/')
        && !pattern.contains(['\\', ':', '[', ']', '{', '}', '\n', '\r', '\0'])
        && !pattern
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

pub(super) fn glob(pattern: &str) -> bool {
    pattern.contains(['*', '?'])
}

pub(super) fn scope(
    document: &Document<'_>,
    declarations: &[Declaration],
    paths: &BTreeSet<String>,
    is_file: impl Fn(&str) -> bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<SourceScope> {
    let mut valid = !declarations.is_empty();
    let mut scope = SourceScope {
        declarations: BTreeSet::new(),
        paths: BTreeSet::new(),
        missing_lines: Vec::new(),
    };
    if declarations.is_empty() {
        diagnostics.push(document.diagnostic(
            document.metadata.body_line,
            "sources",
            "No declared source files",
            "Add a Key Files Path | Purpose table describing relevant files or globs",
        ));
    }
    for entry in declarations {
        if !safe(&entry.path) || entry.purpose.is_empty() || entry.path == document.source.path {
            valid = false;
            diagnostics.push(document.diagnostic(
                entry.line,
                "sources",
                &entry.path,
                "Use a safe, non-self project-relative file/glob and a nonempty purpose",
            ));
            continue;
        }
        if !scope.declarations.insert(DeclaredSource {
            path: entry.path.clone(),
            purpose: entry.purpose.clone(),
        }) {
            valid = false;
            diagnostics.push(document.diagnostic(
                entry.line,
                "sources",
                &entry.path,
                "Remove duplicate source declarations",
            ));
        }
        if document.living() {
            let before = diagnostics.len();
            resolve(
                paths,
                &is_file,
                document,
                entry,
                &mut scope.paths,
                diagnostics,
            );
            if diagnostics.len() != before {
                scope.missing_lines.push(entry.line);
            }
        }
    }
    (valid && document.living()).then_some(scope)
}

fn resolve(
    paths: &BTreeSet<String>,
    is_file: impl Fn(&str) -> bool,
    document: &Document<'_>,
    entry: &Declaration,
    result: &mut BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !glob(&entry.path) {
        if paths.contains(&entry.path) && is_file(&entry.path) {
            result.insert(entry.path.clone());
        } else {
            diagnostics.push(document.diagnostic(
                entry.line,
                "sources",
                &entry.path,
                "Declare an existing, exactly cased source file",
            ));
        }
        return;
    }
    let matched: Vec<_> = paths
        .iter()
        .filter(|path| {
            *path != &document.source.path && matches(&entry.path, path) && is_file(path)
        })
        .collect();
    if matched.is_empty() {
        diagnostics.push(document.diagnostic(
            entry.line,
            "sources",
            &entry.path,
            "Declare an existing, exactly cased file or a glob matching at least one source file",
        ));
    }
    result.extend(matched.into_iter().cloned());
}

pub(super) fn content_hash(context: &mut Context<'_>, path: &str) -> Outcome<String> {
    if let Some(hash) = context.hashes.get(path) {
        return Ok(hash.clone());
    }
    let bytes = fs::read(context.project.root.join(path))?;
    let hash = hash_bytes(&bytes, path);
    context.hashes.insert(path.into(), hash.clone());
    Ok(hash)
}

pub(super) fn hash_bytes(bytes: &[u8], path: &str) -> String {
    format!("{:x}", Sha256::digest(metadata::normalized(bytes, path)))
}

fn field(digest: &mut Sha256, value: &[u8]) {
    digest.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_le_bytes());
    digest.update(value);
}

pub(super) fn affected(pattern: &str, target: &str, directory: bool) -> bool {
    if !safe(pattern) || (!target.is_empty() && !safe(target)) {
        return false;
    }
    if target.is_empty() || pattern == target || matches(pattern, target) {
        return true;
    }
    if !directory {
        return false;
    }
    if !glob(pattern) {
        return inside(pattern, target);
    }
    // A directory can affect the scope even after its last matching file was
    // removed. Match possible glob prefixes instead of walking the full corpus
    // once for every declaration/target pair.
    let parts: Vec<_> = pattern.split('/').collect();
    (1..parts.len()).any(|end| matches(&parts[..end].join("/"), target))
}

#[cfg(test)]
mod tests {
    use super::affected;

    #[test]
    fn directory_targets_include_possible_scopes_even_after_the_last_file_is_deleted() {
        for (pattern, directory) in [
            ("tools/data/*.json", "tools/data"),
            ("assets/**/*.svg", "assets/new"),
            ("src/**/docs/*.md", "src/features/ui"),
            ("src/features/ui/node.gd", "src/features"),
            ("tools/*.json", ""),
        ] {
            assert!(affected(pattern, directory, true), "{pattern}: {directory}");
        }
        assert!(!affected("tools/*.json", "tools/nested", true));
        assert!(!affected("src/*/docs/*.md", "src/features/ui", true));
        assert!(!affected("tools/*.json", "assets", true));
    }

    #[test]
    fn file_targets_do_not_expand_to_other_files_in_the_same_directory() {
        assert!(affected("tools/*.py", "tools/verify.py", false));
        assert!(affected("src/**/*.gd", "src/features/run/model.gd", false));
        assert!(!affected("tools/*.py", "tools/report.json", false));
        assert!(!affected("tools/*.json", "tools/nested/report.json", false));
        assert!(!affected("tools/verify.py", "tools/build.py", false));
        assert!(!affected("tools/*.py", "../tools/verify.py", false));
    }
}
