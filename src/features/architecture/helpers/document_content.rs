mod details;
mod git;
mod markdown;
mod metadata;
mod records;
mod review;
mod sources;

use crate::{
    features::configuration::{models::Project, rules},
    shared::{
        helpers::issue,
        models::{Corpus, Diagnostic, Outcome, Source},
    },
};
use details::{DocChecks, PendingSources};
pub(crate) use git::GitBaseline;
pub(crate) use records::DIRECTORY as RECORD_DIRECTORY;
pub(crate) use review::review;
use sources::{Snapshot, SourceScope};
use std::collections::{BTreeMap, BTreeSet};

enum CurrentSources {
    Complete(Snapshot),
    Missing(SourceScope),
}

struct Context<'a> {
    project: &'a Project,
    corpus: &'a Corpus,
    hashes: BTreeMap<String, String>,
}

struct Document<'a> {
    source: &'a Source,
    role: &'static str,
    metadata: metadata::Metadata,
    markdown: markdown::Markdown,
}

impl<'a> Document<'a> {
    fn parse(source: &'a Source) -> Option<Self> {
        let role = role(&source.path)?;
        let metadata = metadata::Metadata::parse(&source.text);
        let markdown = markdown::Markdown::parse(&source.text, metadata.body_offset);
        Some(Self {
            source,
            role,
            metadata,
            markdown,
        })
    }

    fn living(&self) -> bool {
        !matches!(self.role, "decision" | "verification")
    }

    fn diagnostic(&self, line: usize, rule: &str, actual: &str, expected: &str) -> Diagnostic {
        issue(
            &self.source.path,
            line,
            1,
            &format!("architecture/document-{rule}"),
            "<document>",
            actual,
            expected,
        )
    }
}

pub(crate) fn key_files(source: &Source) -> Vec<markdown::Declaration> {
    Document::parse(source)
        .filter(Document::living)
        .map_or_else(Vec::new, |document| markdown::declarations(&document))
}

pub(crate) fn source_declaration_matches(pattern: &str, file: &str, document: &str) -> bool {
    file != document
        && sources::safe(pattern)
        && if sources::glob(pattern) {
            rules::matches(pattern, file)
        } else {
            pattern == file
        }
}

fn role(path: &str) -> Option<&'static str> {
    let roles = [
        "specification",
        "guide",
        "reference",
        "standard",
        "decision",
        "verification",
    ];
    let stem = path.rsplit('/').next()?.strip_suffix(".md")?;
    roles
        .iter()
        .copied()
        .find(|role| stem.ends_with(&format!("_{role}")))
        .or_else(|| {
            roles.iter().copied().find(|role| {
                let folder = if *role == "verification" {
                    "verification".into()
                } else {
                    format!("{role}s")
                };
                path.contains(&format!("/docs/{folder}/"))
            })
        })
}

pub(super) fn check<'a>(
    project: &'a Project,
    corpus: &'a Corpus,
    targets: &[String],
    diagnostics: &mut Vec<Diagnostic>,
) -> Outcome<DocChecks<'a>> {
    let mut context = Context {
        project,
        corpus,
        hashes: BTreeMap::new(),
    };
    let targets: Vec<_> = targets
        .iter()
        .map(|target| (target.as_str(), project.root.join(target).is_dir()))
        .collect();
    let mut dependents = BTreeSet::new();
    let mut pending = BTreeMap::new();
    for source in &corpus.documents {
        let Some(document) = Document::parse(source) else {
            continue;
        };
        let declarations = markdown::declarations(&document);
        if document.living() && affected(&declarations, &targets) {
            dependents.insert(source.path.clone());
        }
        if let Some(current) = validate(&mut context, &document, &declarations, false, diagnostics)?
            && let Some(change) = PendingSources::detect(&document, current, diagnostics)
        {
            pending.insert(source.path.clone(), change);
        }
    }
    Ok(DocChecks {
        context,
        dependents,
        pending,
    })
}

fn affected(declarations: &[markdown::Declaration], targets: &[(&str, bool)]) -> bool {
    declarations.iter().any(|declaration| {
        targets
            .iter()
            .any(|(target, directory)| sources::affected(&declaration.path, target, *directory))
    })
}

fn validate(
    context: &mut Context<'_>,
    document: &Document<'_>,
    declarations: &[markdown::Declaration],
    reviewing: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Outcome<Option<CurrentSources>> {
    document.validate_metadata(reviewing, diagnostics);
    document.validate_markdown_structure(diagnostics);
    size(context.project, document, diagnostics);
    let Some(scope) = sources::scope(
        document,
        declarations,
        &context.corpus.paths,
        |path| context.project.root.join(path).is_file(),
        diagnostics,
    ) else {
        return Ok(None);
    };
    if scope.missing_lines.is_empty() {
        let snapshot = scope.snapshot(|path| sources::content_hash(context, path))?;
        Ok(Some(CurrentSources::Complete(snapshot)))
    } else {
        Ok(Some(CurrentSources::Missing(scope)))
    }
}

fn size(project: &Project, document: &Document<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let rule = "architecture/document-size";
    let defaults = if document.living() {
        (150, 1500)
    } else {
        (100, 1000)
    };
    let limit = |key, default| {
        rules::threshold_for_path(
            &project.settings.rules,
            &project.settings.overrides,
            rule,
            key,
            &document.source.path,
            default,
        )
    };
    let max_lines = limit("max_lines", defaults.0);
    let max_words = limit("max_words", defaults.1);
    let lines = document.source.text[document.metadata.body_offset..]
        .lines()
        .count();
    let words = document.markdown.words;
    if lines > max_lines || words > max_words {
        diagnostics.push(document.diagnostic(document.metadata.body_line, "size",
            &format!("{lines} body lines / {words} words"),
            &format!("At most {max_lines} body lines / {max_words} words; remove repetition, split focused topics, and link canonical facts or evidence")));
    }
}
