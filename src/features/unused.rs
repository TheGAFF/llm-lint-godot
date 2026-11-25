mod graph;
mod inputs;
mod release;

use crate::{
    features::{
        architecture::helpers::{audit_symbols, corpus},
        configuration::helpers::{discover_settings, inside, target_paths},
    },
    shared::models::Outcome,
};
use std::{cmp::Reverse, collections::BTreeMap, path::Path};

pub(crate) const USAGE: &str =
    "unused [paths...] [--kind asset|file|function|variable] [--json] [--fail-on-findings]";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    Asset,
    File,
    Function,
    Variable,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Asset => "asset",
            Self::File => "file",
            Self::Function => "function",
            Self::Variable => "variable",
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct Finding {
    pub kind: Kind,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    pub line: usize,
    pub column: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub companions: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub(crate) struct Gap {
    pub path: String,
    pub line: usize,
    pub message: String,
    pub kinds: &'static [Kind],
}

pub(crate) const FILE_KINDS: &[Kind] = &[Kind::Asset, Kind::File];
pub(crate) const SYMBOL_KINDS: &[Kind] = &[Kind::Function, Kind::Variable];
pub(crate) const ALL_KINDS: &[Kind] = &[Kind::Asset, Kind::File, Kind::Function, Kind::Variable];

#[derive(serde::Serialize)]
struct Report<'a> {
    version: u8,
    scanned_files: usize,
    entrypoints: Vec<&'a str>,
    counts: BTreeMap<&'static str, usize>,
    diagnostics: Vec<Finding>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    release_exclusions: Vec<release::Advisory>,
    coverage_gaps: Vec<Gap>,
}

#[derive(Default)]
struct Options {
    kind: Option<Kind>,
    paths: Vec<String>,
    json: bool,
    fail: bool,
}

impl Options {
    fn parse(arguments: &[String]) -> Outcome<Self> {
        let mut options = Self::default();
        let mut args = arguments.iter().map(String::as_str);
        while let Some(arg) = args.next() {
            match arg {
                "--json" if !options.json => options.json = true,
                "--fail-on-findings" if !options.fail => options.fail = true,
                "--kind" if options.kind.is_none() => {
                    options.kind = Some(parse_kind(args.next())?);
                }
                option if option.starts_with('-') => {
                    return Err(format!("Unexpected option {option}. Usage: {USAGE}").into());
                }
                path => options.paths.push(path.into()),
            }
        }
        Ok(options)
    }

    fn includes(&self, kind: Kind) -> bool {
        self.kind.is_none_or(|selected| selected == kind)
    }
}

fn parse_kind(value: Option<&str>) -> Outcome<Kind> {
    let label = value.ok_or("--kind requires a value")?;
    ALL_KINDS
        .iter()
        .copied()
        .find(|kind| kind.label() == label)
        .ok_or_else(|| format!("Unknown unused kind: {label}. Usage: {USAGE}").into())
}

pub(crate) fn run(args: &[String], config: Option<&Path>) -> Outcome<i32> {
    if args == ["--help"] || args == ["-h"] {
        println!("Usage: godot-llm-lint {USAGE}");
        return Ok(0);
    }
    let options = Options::parse(args)?;
    let project = discover_settings(&std::env::current_dir()?, config)?;
    let config_name = config
        .and_then(Path::file_name)
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("llm-lint.godot.toml");
    let targets = target_paths(&project, &options.paths)?;
    let corpus = corpus(&project)?;
    let graph = graph::Graph::build(&project, &corpus, config_name)?;
    let mut diagnostics = graph.findings(&options)?;
    let release_exclusions = if options.includes(Kind::Asset) {
        graph.release_advisories(&targets)?
    } else {
        Vec::new()
    };
    let mut coverage_gaps = graph.gaps;
    if SYMBOL_KINDS.iter().any(|&kind| options.includes(kind)) {
        let (symbols, gaps) = audit_symbols(&corpus, &graph.reachable, &project.settings.unused)?;
        diagnostics.extend(symbols);
        coverage_gaps.extend(gaps);
    }
    diagnostics.retain(|finding| {
        options.includes(finding.kind)
            && (targets.is_empty() || targets.iter().any(|target| inside(&finding.path, target)))
    });
    diagnostics.sort_by(|left, right| {
        let key = |finding: &Finding| (finding.kind, Reverse(finding.bytes.unwrap_or(0)));
        key(left)
            .cmp(&key(right))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| {
                (left.line, left.column, &left.item).cmp(&(right.line, right.column, &right.item))
            })
    });
    coverage_gaps.retain(|gap| gap.kinds.iter().any(|&kind| options.includes(kind)));
    coverage_gaps.sort();
    coverage_gaps.dedup();
    let mut counts: BTreeMap<_, _> = ALL_KINDS
        .iter()
        .filter(|&&kind| options.includes(kind))
        .map(|kind| (kind.label(), 0))
        .collect();
    for finding in &diagnostics {
        *counts.entry(finding.kind.label()).or_default() += 1;
    }
    let report = Report {
        version: 1,
        scanned_files: graph.file_count,
        entrypoints: graph.roots.into_iter().collect(),
        counts,
        diagnostics,
        release_exclusions,
        coverage_gaps,
    };
    report.print(options.json)?;
    Ok(if !options.fail {
        0
    } else if !report.coverage_gaps.is_empty() {
        2
    } else {
        i32::from(!report.diagnostics.is_empty())
    })
}

impl Report<'_> {
    fn print(&self, json: bool) -> Outcome<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(self)?);
            return Ok(());
        }
        for finding in &self.diagnostics {
            finding.print();
        }
        for advisory in &self.release_exclusions {
            advisory.print();
        }
        for gap in &self.coverage_gaps {
            eprintln!("{}:{} coverage: {}", gap.path, gap.line, gap.message);
        }
        println!(
            "Unused: {} candidates, {} files, {} entry points, {} coverage gaps.",
            self.diagnostics.len(),
            self.scanned_files,
            self.entrypoints.len(),
            self.coverage_gaps.len()
        );
        if !self.release_exclusions.is_empty() {
            println!(
                "Release exclusions: {} assets to review.",
                self.release_exclusions.len()
            );
        }
        Ok(())
    }
}

impl Finding {
    fn print(&self) {
        let kind = self.kind.label();
        if let Some(item) = &self.item {
            println!("{}:{}:{} {kind}: {item}", self.path, self.line, self.column);
            return;
        }
        let bytes = self
            .bytes
            .map_or_else(String::new, |bytes| format!(" ({bytes} bytes)"));
        let companions = if self.companions.is_empty() {
            String::new()
        } else {
            format!(" [companions: {}]", self.companions.join(", "))
        };
        println!("{} {kind}{bytes}{companions}", self.path);
    }
}
