use super::{Context, CurrentSources, Document, GitBaseline, metadata, records, sources};
use crate::shared::models::{Diagnostic, DocumentSources};
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::features::architecture::helpers) struct DocChecks<'a> {
    pub(super) context: Context<'a>,
    pub(in crate::features::architecture::helpers) dependents: BTreeSet<String>,
    pub(super) pending: BTreeMap<String, PendingSources>,
}

impl DocChecks<'_> {
    pub(in crate::features::architecture::helpers) fn explain(
        mut self,
        diagnostics: &mut [Diagnostic],
        mut git: Option<&mut GitBaseline>,
    ) {
        for diagnostic in diagnostics {
            if !self
                .pending
                .get(&diagnostic.path)
                .is_some_and(|pending| pending.matches(diagnostic))
            {
                continue;
            }
            if let Some(pending) = self.pending.remove(&diagnostic.path) {
                diagnostic.document_sources =
                    Some(pending.explain(&mut self.context, &diagnostic.path, git.as_deref_mut()));
            }
        }
    }
}

pub(super) struct PendingSources {
    recorded: Option<String>,
    current: CurrentSources,
}

impl PendingSources {
    pub(super) fn detect(
        document: &Document<'_>,
        current: CurrentSources,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<Self> {
        let recorded = document
            .metadata
            .sources_hash
            .as_ref()
            .filter(|hash| metadata::valid_hash(hash));
        if let CurrentSources::Complete(current) = &current {
            let recorded = recorded?;
            let fingerprint = current.fingerprint();
            if fingerprint == *recorded {
                return None;
            }
            diagnostics.push(document.diagnostic(document.metadata.hash_line, "stale", recorded,
                &format!("Sources changed ({fingerprint}); review declared files and update the document, then run docs review")));
        }
        Some(Self {
            recorded: recorded.cloned(),
            current,
        })
    }

    fn matches(&self, diagnostic: &Diagnostic) -> bool {
        match &self.current {
            CurrentSources::Complete(_) => diagnostic.rule == "architecture/document-stale",
            CurrentSources::Missing(scope) => {
                diagnostic.rule == "architecture/document-sources"
                    && scope.missing_lines.contains(&diagnostic.line)
            }
        }
    }

    fn explain(
        self,
        context: &mut Context<'_>,
        path: &str,
        git: Option<&mut GitBaseline>,
    ) -> DocumentSources {
        let Some(recorded) = self.recorded else {
            return unavailable("The document has no valid recorded sources_hash");
        };
        let source = match baseline(context, path, &recorded, git) {
            Ok(source) => source,
            Err(reason) => return unavailable(reason),
        };
        let current = match self.current {
            CurrentSources::Complete(snapshot) => Ok(snapshot),
            CurrentSources::Missing(scope) => {
                scope.snapshot(|path| sources::content_hash(context, path))
            }
        };
        let Ok(current) = current else {
            return unavailable("Could not read one or more current declared source files");
        };
        match source {
            (reviewed, Some(commit)) => DocumentSources::Git {
                commit,
                changes: current.changes_since(&reviewed),
            },
            (reviewed, None) => DocumentSources::ReviewRecord {
                changes: current.changes_since(&reviewed),
            },
        }
    }
}

fn baseline(
    context: &Context<'_>,
    path: &str,
    recorded: &str,
    git: Option<&mut GitBaseline>,
) -> Result<(sources::Snapshot, Option<String>), String> {
    let missing = match records::read(context.project, path) {
        Ok(Some(reviewed)) if reviewed.verifies(recorded) => return Ok((reviewed, None)),
        Ok(Some(_)) => "The review companion does not reproduce the recorded sources_hash",
        Ok(None) => "No review companion is available",
        Err(_) => "The review companion could not be read or is invalid",
    };
    let Some(git) = git else {
        return Err(format!(
            "{missing}. Supply --doc-baseline with the reviewed Git revision."
        ));
    };
    let reviewed = git
        .snapshot(context.project, path)
        .map_err(|error| format!("{missing}. Could not reconstruct the Git baseline: {error}"))?;
    if !reviewed.verifies(recorded) {
        return Err(format!(
            "{missing}. The Git baseline does not reproduce the recorded sources_hash."
        ));
    }
    Ok((reviewed, Some(git.commit.clone())))
}

fn unavailable(reason: impl Into<String>) -> DocumentSources {
    DocumentSources::Unavailable {
        reason: reason.into(),
    }
}
