use super::{Finding, inputs};
use crate::{
    features::configuration::{helpers::safe_path, models::Project},
    shared::models::Outcome,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub(super) struct Retention {
    pub(super) kind: RetentionKind,
    pub(super) path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RetentionKind {
    ReleaseExclusion,
    RuntimeReference,
    ProjectReference,
}

impl RetentionKind {
    fn label(self) -> &'static str {
        match self {
            Self::ReleaseExclusion => "release exclusion",
            Self::RuntimeReference => "runtime reference",
            Self::ProjectReference => "project reference",
        }
    }
}

#[derive(serde::Serialize)]
pub(super) struct Advisory {
    #[serde(flatten)]
    pub(super) file: Finding,
    pub(super) reachable_from_runtime: bool,
    pub(super) retained_by: BTreeSet<Retention>,
}

impl Advisory {
    pub(super) fn print(&self) {
        let status = if self.reachable_from_runtime {
            "runtime reachable; review exclusion"
        } else {
            "outside observed runtime graph"
        };
        println!(
            "{} release exclusion ({} bytes, {status})",
            self.file.path,
            self.file.bytes.unwrap_or(0)
        );
        for reference in &self.retained_by {
            let kind = reference.kind.label();
            if let Some(reason) = &reference.reason {
                println!("  {kind} in {}: {reason}", reference.path);
            } else {
                println!("  {kind} in {}", reference.path);
            }
        }
        if !self.file.companions.is_empty() {
            println!("  Companions: {}", self.file.companions.join(", "));
        }
    }
}

#[derive(serde::Deserialize)]
struct Manifest {
    asset_exclusions: Vec<Entry>,
}

#[derive(serde::Deserialize)]
struct Entry {
    path: String,
    reason: String,
}

pub(super) type Index<'a> = BTreeMap<&'a str, BTreeSet<Retention>>;

pub(super) fn load<'a>(project: &Project, files: &BTreeSet<&'a str>) -> Outcome<Index<'a>> {
    let mut index = Index::new();
    for manifest in &project.settings.unused.release_exclusion_files {
        if !files.contains(manifest.as_str()) {
            continue;
        }
        let text = inputs::read(&project.root, manifest)?;
        let manifest_data: Manifest = serde_json::from_str(&text)
            .map_err(|error| format!("Invalid release exclusions in {manifest}: {error}"))?;
        for entry in manifest_data.asset_exclusions {
            safe_path(&entry.path)?;
            if let Some(&path) = files.get(entry.path.as_str()) {
                index.entry(path).or_default().insert(Retention {
                    kind: RetentionKind::ReleaseExclusion,
                    path: manifest.clone(),
                    reason: Some(entry.reason),
                });
            }
        }
    }
    Ok(index)
}
