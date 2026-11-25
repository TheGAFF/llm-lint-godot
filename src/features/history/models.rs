use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub(super) fn from_level(level: &str) -> Self {
        if level == "error" {
            Self::Error
        } else {
            Self::Warning
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

pub(super) struct Finding {
    pub rule: String,
    pub severity: Severity,
    pub locations: Vec<Location>,
}

pub(super) struct Location {
    pub path: String,
    pub line: Option<i64>,
    pub column: Option<i64>,
}

impl Location {
    pub(super) fn new(path: &str, line: Option<i64>, column: Option<i64>) -> Option<Self> {
        if path.contains('\\')
            || Path::new(path)
                .components()
                .any(|part| matches!(part, Component::Prefix(_)))
            || path.split('/').any(|part| matches!(part, "" | "." | ".."))
        {
            return None;
        }
        Some(Self {
            path: path.into(),
            line: line.filter(|number| *number > 0),
            column: column.filter(|number| *number > 0),
        })
    }

    pub(super) fn display(&self) -> String {
        match (self.line, self.column) {
            (Some(line), Some(column)) => format!("{}:{line}:{column}", self.path),
            (Some(line), None) => format!("{}:{line}", self.path),
            _ => self.path.clone(),
        }
    }
}

pub(crate) enum Stage {
    Configuration,
    Architecture,
    Native,
    Duplicates,
}

impl Stage {
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Architecture => "architecture",
            Self::Native => "native",
            Self::Duplicates => "duplicates",
        }
    }
}

pub(crate) struct Capture {
    pub(crate) stage: Stage,
    pub(super) path: PathBuf,
    pub(super) model: String,
    pub(super) baseline: Option<i64>,
    pub(super) policy: String,
    pub(super) started_at: i64,
    pub(super) findings: Vec<Finding>,
}

pub(crate) struct RecordOptions {
    pub(super) model: String,
    pub(super) baseline: Option<i64>,
}

pub(crate) struct ReportOptions {
    pub(super) model: Option<String>,
    pub(super) since: Option<String>,
    pub(super) run: Option<i64>,
    pub(crate) html: Option<PathBuf>,
}

pub(super) struct Run {
    pub id: i64,
    pub model: String,
    pub finished: String,
    pub exit_code: i32,
    pub version: String,
    pub policy: String,
    pub baseline: Option<i64>,
    pub failure_stage: Option<String>,
    pub selected: bool,
    pub findings: Vec<Finding>,
}

pub(crate) struct Report {
    pub(super) groups: Vec<Group>,
    pub(super) detail: bool,
}

pub(super) struct Group {
    pub version: String,
    pub policy: String,
    pub tables: Vec<Table>,
    pub runs: Vec<Run>,
}

pub(super) struct Table {
    pub title: &'static str,
    pub columns: &'static [&'static str],
    pub rows: Vec<Vec<String>>,
}
