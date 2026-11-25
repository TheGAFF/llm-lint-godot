use super::{
    super::models::{Capture, Finding, Location, RecordOptions, Severity, Stage},
    database,
};
use crate::{
    features::{
        configuration::models::Project, duplicates::models::DuplicateReport,
        execution::models::NativeFinding,
    },
    shared::models::{ArchitectureReport, Outcome},
};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

impl Capture {
    pub(crate) fn begin(project: &Project, options: RecordOptions) -> Outcome<Self> {
        let path = database::history_path(&project.root)?;
        let mut settings = serde_json::to_value(&project.settings)?;
        if let Some(native) = settings["gdstyle"].as_object_mut() {
            native.remove("executable");
        }
        let policy = format!("{:x}", Sha256::digest(serde_json::to_vec(&settings)?));
        if let Some(id) = options.baseline {
            database::baseline_status(&path, id, &policy)?;
        }
        Ok(Self {
            stage: Stage::Configuration,
            path,
            model: options.model,
            baseline: options.baseline,
            policy,
            started_at: timestamp()?,
            findings: Vec::new(),
        })
    }

    pub(crate) fn architecture(&mut self, report: &ArchitectureReport) {
        self.findings.extend(report.diagnostics.iter().map(|item| {
            Finding {
                rule: item.rule.clone(),
                severity: Severity::from_level(&item.severity),
                locations: Location::new(
                    &item.path,
                    i64::try_from(item.line).ok(),
                    i64::try_from(item.column).ok(),
                )
                .into_iter()
                .collect(),
            }
        }));
    }

    pub(crate) fn native(&mut self, finding: NativeFinding<'_>) {
        self.findings.push(Finding {
            rule: finding.rule.into(),
            severity: Severity::from_level(finding.level),
            locations: Location::new(finding.path, finding.line, finding.column)
                .into_iter()
                .collect(),
        });
    }

    pub(crate) fn duplicates(&mut self, report: &DuplicateReport) {
        self.findings
            .extend(report.diagnostics.iter().map(|finding| {
                Finding {
                    rule: finding.rule.clone(),
                    severity: Severity::from_level(&finding.severity),
                    locations: finding
                        .occurrences
                        .iter()
                        .filter_map(|item| {
                            Location::new(
                                &item.path,
                                i64::try_from(item.start.line).ok(),
                                i64::try_from(item.start.column).ok(),
                            )
                        })
                        .collect(),
                }
            }));
    }
}

pub(super) fn timestamp() -> Outcome<i64> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    )?)
}
