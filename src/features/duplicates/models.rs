use crate::features::configuration::models::DuplicateSettings;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Production,
    Test,
    Capture,
}

impl Category {
    pub(crate) const ALL: [Self; 3] = [Self::Production, Self::Test, Self::Capture];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Test => "test",
            Self::Capture => "capture",
        }
    }

    pub(super) fn from_path(path: &str) -> Self {
        let (directories, filename) = path.rsplit_once('/').unwrap_or(("", path));
        if directories
            .split('/')
            .any(|part| matches!(part, "capture" | "captures"))
            || filename.starts_with("capture_")
            || filename.ends_with("_capture.gd")
            || filename.ends_with("_capture_test_harness.gd")
        {
            Self::Capture
        } else if directories
            .split('/')
            .any(|part| matches!(part, "test" | "tests" | "testing"))
            || filename.starts_with("test_")
            || filename.ends_with("_test_harness.gd")
            || filename.ends_with("_test_factory.gd")
        {
            Self::Test
        } else {
            Self::Production
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct WarningCounts {
    pub production: usize,
    pub test: usize,
    pub capture: usize,
}

impl WarningCounts {
    pub(super) fn add(&mut self, category: Category) {
        match category {
            Category::Production => self.production += 1,
            Category::Test => self.test += 1,
            Category::Capture => self.capture += 1,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Position {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Occurrence {
    pub category: Category,
    pub fingerprint: String,
    pub path: String,
    pub item: String,
    pub start: Position,
    pub end: Position,
    pub tokens: usize,
    pub code_lines: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Guidance {
    pub kind: String,
    pub destination: Option<String>,
    pub message: String,
    pub blockers: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub category: Category,
    pub rule: String,
    pub kind: String,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity: Option<f64>,
    pub occurrences: Vec<Occurrence>,
    pub guidance: Guidance,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub scanned_files: usize,
    pub analyzed_units: usize,
    pub errors: usize,
    pub warnings: usize,
    pub warnings_by_category: WarningCounts,
    pub suppressed: usize,
}

#[derive(Debug, Serialize)]
pub struct SuppressedFinding {
    pub exception_id: String,
    pub reason: String,
    pub finding: Finding,
}

#[derive(Debug, Serialize)]
pub struct DuplicateReport {
    pub version: u8,
    pub engine: &'static str,
    pub thresholds: DuplicateSettings,
    pub summary: Summary,
    pub diagnostics: Vec<Finding>,
    pub suppressed: Vec<SuppressedFinding>,
}

#[derive(Clone, Debug)]
pub struct Match {
    pub left: usize,
    pub right: usize,
    pub left_start: usize,
    pub right_start: usize,
    pub length: usize,
    pub kind: &'static str,
    pub similarity: Option<f64>,
}
