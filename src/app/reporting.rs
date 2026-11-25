use crate::{
    features::duplicates::models::{Category, DuplicateReport, Finding},
    shared::models::{ArchitectureReport, DocumentSources, Outcome},
};

#[derive(Clone, Copy)]
pub(super) enum DuplicateWarnings {
    All,
    Category(Category),
}

impl DuplicateWarnings {
    pub(super) fn parse(value: &str) -> Outcome<Self> {
        if value == "all" {
            return Ok(Self::All);
        }
        Category::ALL
            .into_iter()
            .find(|category| category.label() == value)
            .map(Self::Category)
            .ok_or_else(|| "--duplicate-warnings expects production, test, capture, or all".into())
    }

    fn includes(self, category: Category) -> bool {
        match self {
            Self::All => true,
            Self::Category(selected) => selected == category,
        }
    }
}

pub(super) fn print_architecture(report: &ArchitectureReport, json: bool) -> Outcome<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for diagnostic in &report.diagnostics {
            eprintln!(
                "{}:{}:{} {} [{}]: {} -> {}",
                diagnostic.path,
                diagnostic.line,
                diagnostic.column,
                diagnostic.rule,
                diagnostic.item,
                diagnostic.actual,
                diagnostic.expected
            );
            if let Some(details) = &diagnostic.document_sources {
                print_sources(details);
            }
        }
        println!(
            "Architecture: {} files, {} errors.",
            report.scanned_files, report.errors
        );
    }
    Ok(())
}

fn print_sources(details: &DocumentSources) {
    let changes = match details {
        DocumentSources::ReviewRecord { changes } => {
            eprintln!("  Compared with the verified review companion.");
            changes
        }
        DocumentSources::Git { commit, changes } => {
            eprintln!("  Compared with verified Git commit {commit}.");
            changes
        }
        DocumentSources::Unavailable { reason } => {
            eprintln!("  Source attribution unavailable. {reason}");
            return;
        }
    };
    if changes.declarations_changed {
        eprintln!("  Key Files declarations changed.");
    }
    for (label, paths) in [
        ("Modified", &changes.modified),
        ("Newly included", &changes.added),
        ("No longer included", &changes.removed),
    ] {
        for path in paths {
            eprintln!("  {label}: {path}");
        }
    }
}

pub(super) fn print_duplicates(
    report: &DuplicateReport,
    json: bool,
    warnings: Option<DuplicateWarnings>,
) -> Outcome<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    println!(
        "Duplicates: {} units, {} errors, {} warnings, {} reviewed findings.",
        report.summary.analyzed_units,
        report.summary.errors,
        report.summary.warnings,
        report.summary.suppressed
    );
    let counts = &report.summary.warnings_by_category;
    println!(
        "Duplicate warnings: {} production, {} test, {} capture.",
        counts.production, counts.test, counts.capture
    );
    for finding in report
        .diagnostics
        .iter()
        .filter(|finding| finding.severity == "error")
    {
        eprintln!("{}: {}", finding.rule, locations(finding, false));
    }
    if let Some(warnings) = warnings {
        print_duplicate_warnings(report, warnings);
    }
    Ok(())
}

fn print_duplicate_warnings(report: &DuplicateReport, warnings: DuplicateWarnings) {
    for category in Category::ALL
        .into_iter()
        .filter(|&category| warnings.includes(category))
    {
        let mut group = report
            .diagnostics
            .iter()
            .filter(|finding| finding.severity == "warning" && finding.category == category)
            .peekable();
        if group.peek().is_some() {
            eprintln!("Duplicate warnings [{}]", category.label());
            for finding in group {
                eprintln!("  {}: {}", finding.rule, locations(finding, true));
            }
        }
    }
}

fn locations(finding: &Finding, labels: bool) -> String {
    finding
        .occurrences
        .iter()
        .map(|occurrence| {
            let location = format!("{}:{}", occurrence.path, occurrence.start.line);
            if labels {
                format!("{location} [{}]", occurrence.category.label())
            } else {
                location
            }
        })
        .collect::<Vec<_>>()
        .join(" <-> ")
}
