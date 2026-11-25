use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReasonedPath {
    pub path: String,
    pub reason: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExcludedPath {
    pub path: String,
    pub reason: String,
    #[serde(default)]
    pub optional: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_discovery: bool,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DynamicLoad {
    pub path: String,
    pub item: String,
    pub roots: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    pub reason: String,
}

impl DynamicLoad {
    pub fn matches_extension(&self, path: &str) -> bool {
        self.extensions.is_empty()
            || path.rsplit_once('.').is_some_and(|(_, extension)| {
                self.extensions.iter().any(|allowed| extension == allowed)
            })
    }
}

#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UnusedSettings {
    pub reference_files: Vec<String>,
    pub release_exclusion_files: Vec<String>,
    pub keep: Vec<UnusedKeep>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct UnusedKeep {
    pub path: String,
    pub item: Option<String>,
    pub reason: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssetClassification {
    pub includes: Vec<String>,
    pub role: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentationExclusion {
    pub includes: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Exception {
    pub rule: String,
    pub path: String,
    pub item: String,
    pub reason: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct BundleSettings {
    pub owners: Vec<String>,
    pub max_files: usize,
    pub companions: Vec<BundleCompanions>,
    pub exclusions: Vec<BundleExclusion>,
}

impl Default for BundleSettings {
    fn default() -> Self {
        Self {
            owners: Vec::new(),
            max_files: 30,
            companions: Vec::new(),
            exclusions: Vec::new(),
        }
    }
}

impl BundleSettings {
    fn inactive(&self) -> bool {
        self.owners.is_empty() && self.companions.is_empty() && self.exclusions.is_empty()
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct BundleCompanions {
    pub anchor: String,
    pub paths: Vec<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct BundleExclusion {
    pub path: String,
    pub rule: String,
    pub reason: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateMember {
    pub path: String,
    pub item: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateException {
    pub id: String,
    pub rules: Vec<String>,
    pub reason: String,
    pub members: Vec<DuplicateMember>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTool {
    #[serde(default)]
    pub auto: bool,
    #[serde(default)]
    pub executable: String,
    pub version: String,
    #[serde(default)]
    pub sha256: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DuplicateSettings {
    pub enabled: bool,
    pub warning_tokens: usize,
    pub warning_lines: usize,
    pub error_tokens: usize,
    pub error_lines: usize,
    pub similarity: f64,
}

impl Default for DuplicateSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            warning_tokens: 50,
            warning_lines: 5,
            error_tokens: 100,
            error_lines: 10,
            similarity: 0.85,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    #[serde(skip_serializing_if = "BundleSettings::inactive")]
    pub bundles: BundleSettings,
    pub asset_classifications: Vec<AssetClassification>,
    pub documentation_exclusions: Vec<DocumentationExclusion>,
    pub rules: std::collections::BTreeMap<String, RuleSetting>,
    pub overrides: Vec<RuleOverride>,
    pub version: u8,
    pub source_roots: Vec<String>,
    pub plugin_roots: Vec<String>,
    pub tooling_roots: Vec<String>,
    pub excluded: Vec<ExcludedPath>,
    pub entrypoints: Vec<ReasonedPath>,
    pub dynamic_loads: Vec<DynamicLoad>,
    pub exceptions: Vec<Exception>,
    pub duplicate_exceptions: Vec<DuplicateException>,
    pub collection_max_code_lines: usize,
    pub style: BTreeMap<String, usize>,
    pub duplicates: DuplicateSettings,
    pub unused: UnusedSettings,
    pub gdstyle: Option<NativeTool>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bundles: BundleSettings::default(),
            asset_classifications: vec![],
            documentation_exclusions: vec![],
            rules: std::collections::BTreeMap::new(),
            overrides: Vec::new(),
            version: 1,
            source_roots: vec!["src".into()],
            plugin_roots: vec![],
            tooling_roots: vec![],
            excluded: vec![],
            entrypoints: vec![],
            dynamic_loads: vec![],
            exceptions: vec![],
            duplicate_exceptions: vec![],
            collection_max_code_lines: 300,
            style: BTreeMap::new(),
            duplicates: DuplicateSettings::default(),
            unused: UnusedSettings::default(),
            gdstyle: None,
        }
    }
}

#[derive(Debug)]
pub struct Project {
    pub root: PathBuf,
    pub settings: Settings,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum RuleSetting {
    Level(String),
    Detailed(RuleOptions),
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuleOptions {
    pub level: Option<String>,
    #[serde(default)]
    pub options: BTreeMap<String, serde_json::Value>,
}

impl RuleSetting {
    pub fn level(&self) -> Option<&str> {
        match self {
            Self::Level(level) => Some(level),
            Self::Detailed(value) => value.level.as_deref(),
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuleOverride {
    pub includes: Vec<String>,
    #[serde(default)]
    pub rules: BTreeMap<String, RuleSetting>,
}

#[derive(serde::Deserialize)]
pub struct Release {
    pub target: String,
    pub url: String,
    pub archive_sha256: String,
    pub executable_sha256: String,
    pub archive_name: String,
    pub executable: String,
}
