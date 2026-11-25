pub type Outcome<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Debug, serde::Serialize)]
pub struct Diagnostic {
    pub severity: String,
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub rule: String,
    pub actual: String,
    pub expected: String,
    pub item: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_sources: Option<DocumentSources>,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "baseline", rename_all = "snake_case")]
pub enum DocumentSources {
    ReviewRecord {
        #[serde(flatten)]
        changes: SourceChanges,
    },
    Git {
        commit: String,
        #[serde(flatten)]
        changes: SourceChanges,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct SourceChanges {
    pub modified: Vec<String>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub declarations_changed: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct ArchitectureReport {
    pub version: u8,
    pub mode: &'static str,
    pub scanned_files: usize,
    pub violating_files: usize,
    pub errors: usize,
    pub warnings: usize,
    pub counts: std::collections::BTreeMap<String, usize>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug)]
pub struct Token {
    /// Lexical local/parameter binding; members, APIs and types never acquire one.
    pub binding: Option<usize>,
    pub text: String,
    pub kind: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub end_line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub fingerprint: String,
    pub semantic: Vec<String>,
    pub operators: Vec<String>,
    pub data_only: bool,
    pub name: String,
    pub static_method: bool,
    pub line: usize,
    pub column: usize,
    pub tokens: Vec<Token>,
    pub shape: Vec<String>,
    pub blockers: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum ReferenceOrigin {
    Literal {
        offset: usize,
    },
    Extends,
    Load {
        argument: (usize, usize),
        literal: Option<(usize, usize)>,
        callee: LoadCallee,
    },
    Serialized,
}

#[derive(Clone, Copy, Debug)]
pub enum LoadCallee {
    Load,
    Preload,
    ResourceLoader,
}

#[derive(Clone, Debug)]
pub struct Reference {
    pub origin: ReferenceOrigin,
    pub target: String,
    pub item: String,
    pub line: usize,
    pub column: usize,
    pub dynamic: bool,
    pub directory: bool,
    pub required: bool,
}

#[derive(Debug)]
pub struct Source {
    pub path: String,
    pub text: String,
    pub owner: String,
    pub layer: String,
    pub testing: bool,
    pub class_name: Option<String>,
    pub base: Option<String>,
    pub functions: Vec<Function>,
    pub references: Vec<Reference>,
    pub class_uses: Vec<(String, usize, usize)>,
    pub diagnostics: Vec<Diagnostic>,
    pub code_lines: usize,
    pub instance_state: bool,
    pub native_suppressions: Vec<(usize, String)>,
}

#[derive(Debug)]
pub struct Corpus {
    pub sources: Vec<Source>,
    /// Markdown text and owner metadata; never enters runtime or duplicate analysis.
    pub documents: Vec<Source>,
    pub excluded_sources: Vec<Source>,
    pub paths: std::collections::BTreeSet<String>,
    pub empty_directories: std::collections::BTreeSet<String>,
    pub uids: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, serde::Deserialize)]
pub struct Role(pub String, pub String, pub String, pub bool);
