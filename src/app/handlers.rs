use super::helpers::{explicit_config, history_report};
use super::reporting::{DuplicateWarnings, print_architecture, print_duplicates};
use super::timings::Timings;
use crate::{
    features::{
        architecture::helpers::{
            analyze_parsed_corpus, corpus, document_content::GitBaseline, parsed_corpus,
        },
        configuration::{
            commands::sync_project,
            helpers::{discover_project, inside, native::NativeOverride, target_paths},
            models::Project,
        },
        duplicates::helpers::analyze_duplicate_corpus,
        execution::helpers::native_checks_with,
        history::models::{Capture, RecordOptions, Stage},
    },
    shared::models::Outcome,
};

#[derive(Clone, Copy)]
struct ReportOptions<'a> {
    json: bool,
    duplicate_warnings: Option<DuplicateWarnings>,
    native: Option<&'a NativeOverride>,
}

struct CheckResult {
    status: i32,
    json: Option<serde_json::Value>,
}

impl CheckResult {
    fn finish(self) -> Outcome<i32> {
        if let Some(report) = self.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Ok(self.status)
    }
}

pub fn handle_cli(args: Vec<String>) -> i32 {
    let json = args.iter().any(|argument| argument == "--json");
    match cli(args) {
        Ok(status) => status,
        Err(error) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"version":1, "fatal":error.to_string()})
                );
            } else {
                eprintln!("godot-llm-lint failed: {error}");
            }
            2
        }
    }
}

fn list_rules(args: &[String]) -> Outcome<i32> {
    if !args.is_empty() && args != ["--json"] {
        return Err("Usage: rules [--json]".into());
    }
    let catalog = crate::features::configuration::rules::catalog()?;
    if args == ["--json"] {
        println!("{}", serde_json::to_string_pretty(&catalog)?);
    } else {
        for rule in catalog {
            println!(
                "{} [{}]: {}",
                rule["id"].as_str().unwrap_or(""),
                rule["default"].as_str().unwrap_or(""),
                rule["description"].as_str().unwrap_or("")
            );
        }
    }
    Ok(0)
}

fn cli(mut args: Vec<String>) -> Outcome<i32> {
    if args == ["--version"] {
        println!("godot-llm-lint {}", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }
    if args.is_empty() || ["help", "--help", "-h"].contains(&args[0].as_str()) {
        println!(
            "godot-llm-lint init [--source-root DIR] [--gdstyle-archive FILE]\n  godot-llm-lint sync [--check] | lint [paths...] | check [paths...] [--json] | architecture [paths...] [--json] | duplicates [paths...] [--json] | docs review <explicit-md-paths...> --reviewed YYYY-MM-DD\n  {}\n  godot-llm-lint {}\n  check --record --model LABEL [--baseline-run ID]\n  history report [--model LABEL] [--since YYYY-MM-DD] [--run ID] [--html FILE]\nGlobal option: --config llm-lint.godot.toml. Targets resolve from its directory.",
            crate::features::unused::USAGE,
            crate::features::references::USAGE
        );
        println!(
            "Use --timings with check, lint, architecture, or duplicates for phase timings on stderr."
        );
        println!(
            "Use --doc-baseline <ref> with architecture, check, or lint to explain legacy reviews."
        );
        println!(
            "Use --duplicate-warnings production|test|capture|all with duplicates, check, or lint for terminal warning details."
        );
        println!(
            "Trusted local override for init, check, and lint: --gdstyle-executable PATH [--gdstyle-sha256 DIGEST]"
        );
        return Ok(0);
    }
    let command = args.remove(0);
    if command == "init" {
        crate::features::configuration::initialize::sync_initialization(
            &std::env::current_dir()?,
            &args,
        )?;
        println!("Initialized Godot lint configuration. Run godot-llm-lint check.");
        return Ok(0);
    }
    if command == "rules" {
        return list_rules(&args);
    }
    if command == "docs" {
        return review_documents(args);
    }
    if command == "history" {
        return history_report(args);
    }
    if command == "unused" {
        let explicit = explicit_config(&mut args)?;
        return crate::features::unused::run(&args, explicit.as_deref());
    }
    if command == "references" {
        let explicit = explicit_config(&mut args)?;
        return crate::features::references::run(&args, explicit.as_deref());
    }
    analysis_command(&command, args)
}

fn analysis_command(command: &str, mut args: Vec<String>) -> Outcome<i32> {
    if !["sync", "lint", "check", "architecture", "duplicates"].contains(&command) {
        return Err("Unknown command; run godot-llm-lint --help".into());
    }
    let explicit = explicit_config(&mut args)?;
    let doc_ref = value_option(&mut args, "--doc-baseline")?;
    if doc_ref.is_some() && !matches!(command, "architecture" | "check" | "lint") {
        return Err("--doc-baseline requires architecture, check, or lint".into());
    }
    let json = ["architecture", "check", "duplicates"].contains(&command)
        && args.iter().any(|argument| argument == "--json");
    if args.iter().filter(|argument| *argument == "--json").count() > 1 {
        return Err("Repeated --json".into());
    }
    if json {
        args.retain(|argument| argument != "--json");
    }
    let native = native_override(command, &mut args)?;
    let output = report_options(command, &mut args, json, native.as_ref())?;
    let timings_count = args
        .iter()
        .filter(|argument| *argument == "--timings")
        .count();
    if timings_count > 1 {
        return Err("Repeated --timings".into());
    }
    let mut timings = Timings::new(command != "sync" && timings_count == 1);
    if command != "sync" {
        args.retain(|argument| argument != "--timings");
    }
    let recording = if command == "check" {
        RecordOptions::parse(&mut args)?
    } else {
        None
    };
    let sync_check = command == "sync" && args == ["--check"];
    if command == "sync" && !args.is_empty() && !sync_check {
        return Err("Usage: sync [--check]".into());
    }
    if !sync_check && args.iter().any(|argument| argument.starts_with('-')) {
        return Err("Unsupported option; checks never accept write flags".into());
    }
    let project = timings.measure("discovery", || {
        discover_project(&std::env::current_dir()?, explicit.as_deref())
    })?;
    let mut doc_baseline = doc_ref
        .as_deref()
        .map(|reference| GitBaseline::resolve(&project, reference))
        .transpose()?;
    if command == "sync" {
        sync_project(&project, sync_check)?;
        println!(
            "{}",
            if sync_check {
                "Configuration is fresh."
            } else {
                "Synchronized gdstyle.toml."
            }
        );
        return Ok(0);
    }
    if let Some(options) = recording {
        return recorded_check(
            &project,
            options,
            output,
            &mut timings,
            doc_baseline.as_mut(),
        );
    }
    if matches!(command, "lint" | "check") {
        return check_project(
            &project,
            &args,
            command == "check",
            output,
            None,
            &mut timings,
            doc_baseline.as_mut(),
        )?
        .finish();
    }
    timings.measure("configuration", || sync_project(&project, true))?;
    if command == "duplicates" {
        if !project.settings.duplicates.enabled {
            return Err("Duplicate detection is disabled".into());
        }
        let corpus = timings.measure("corpus", || corpus(&project))?;
        let report = timings.measure("duplicates", || {
            analyze_duplicate_corpus(&project, &corpus, &args)
        })?;
        print_duplicates(&report, json, output.duplicate_warnings)?;
        return Ok(i32::from(report.summary.errors > 0));
    }
    let (corpus, trees) = timings.measure("corpus", || parsed_corpus(&project))?;
    let report = timings.measure("architecture", || {
        analyze_parsed_corpus(&project, &corpus, &trees, &args, doc_baseline.as_mut())
    })?;
    print_architecture(&report, json)?;
    Ok(i32::from(report.errors > 0))
}

fn native_override(command: &str, args: &mut Vec<String>) -> Outcome<Option<NativeOverride>> {
    let executable = value_option(args, "--gdstyle-executable")?;
    let digest = value_option(args, "--gdstyle-sha256")?;
    if (executable.is_some() || digest.is_some()) && !matches!(command, "check" | "lint") {
        return Err("Native executable overrides require init, check, or lint".into());
    }
    NativeOverride::from_arguments(&std::env::current_dir()?, executable, digest)
}

fn value_option(args: &mut Vec<String>, name: &str) -> Outcome<Option<String>> {
    let mut matches = args
        .iter()
        .enumerate()
        .filter(|(_, arg)| arg.as_str() == name);
    let Some((index, _)) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some() {
        return Err(format!("Repeated {name}").into());
    }
    if args
        .get(index + 1)
        .is_none_or(|value| value.is_empty() || value.starts_with('-'))
    {
        return Err(format!("{name} requires a value").into());
    }
    args.remove(index);
    Ok(Some(args.remove(index)))
}

fn report_options<'a>(
    command: &str,
    args: &mut Vec<String>,
    json: bool,
    native: Option<&'a NativeOverride>,
) -> Outcome<ReportOptions<'a>> {
    let duplicate_warnings = value_option(args, "--duplicate-warnings")?
        .as_deref()
        .map(DuplicateWarnings::parse)
        .transpose()?;
    if duplicate_warnings.is_some() && (json || !matches!(command, "duplicates" | "check" | "lint"))
    {
        return Err(
            "--duplicate-warnings requires terminal output from duplicates, check, or lint".into(),
        );
    }
    Ok(ReportOptions {
        json,
        duplicate_warnings,
        native,
    })
}

fn recorded_check(
    project: &Project,
    options: RecordOptions,
    output: ReportOptions<'_>,
    timings: &mut Timings,
    doc_baseline: Option<&mut GitBaseline>,
) -> Outcome<i32> {
    let mut capture = Capture::begin(project, options)?;
    let result = check_project(
        project,
        &[],
        true,
        output,
        Some(&mut capture),
        timings,
        doc_baseline,
    );
    let exit_code = result.as_ref().map_or(2, |check| check.status);
    let id = capture.save(exit_code).map_err(|error| match &result {
        Ok(_) => format!("Could not record lint history: {error}"),
        Err(lint_error) => format!("{lint_error}. Could not record lint history: {error}"),
    })?;
    eprintln!("Recorded lint run {id}.");
    result?.finish()
}

fn check_project(
    project: &Project,
    targets: &[String],
    formatting: bool,
    output: ReportOptions<'_>,
    mut history: Option<&mut Capture>,
    timings: &mut Timings,
    doc_baseline: Option<&mut GitBaseline>,
) -> Outcome<CheckResult> {
    let json = output.json;
    timings.measure("configuration", || sync_project(project, true))?;
    if let Some(capture) = history.as_deref_mut() {
        capture.stage = Stage::Architecture;
    }
    let (corpus, trees) = timings.measure("corpus", || parsed_corpus(project))?;
    let report = timings.measure("architecture", || {
        analyze_parsed_corpus(project, &corpus, &trees, targets, doc_baseline)
    })?;
    drop(trees);
    let mut status = i32::from(report.errors > 0);
    if let Some(capture) = history.as_deref_mut() {
        capture.architecture(&report);
        capture.stage = Stage::Native;
    }
    if !json {
        print_architecture(&report, false)?;
    }
    let selected = target_paths(project, targets)?;
    let files = corpus
        .sources
        .iter()
        .filter(|source| {
            source.path.ends_with(".gd")
                && (selected.is_empty()
                    || selected.iter().any(|target| inside(&source.path, target)))
        })
        .map(|source| source.path.clone())
        .collect::<Vec<_>>();
    let mut native = Vec::new();
    let native_status = timings.measure("native", || {
        native_checks_with(
            project,
            &files,
            formatting,
            json,
            output.native,
            &mut |finding| {
                if let Some(capture) = history.as_deref_mut() {
                    capture.native(finding);
                }
                if json {
                    native.push(serde_json::json!({
                        "rule": finding.rule,
                        "severity": if finding.level == "warn" { "warning" } else { "error" },
                        "path": finding.path,
                        "line": finding.line,
                        "column": finding.column,
                    }));
                }
            },
        )
    })?;
    status = status.max(native_status);
    let duplicates = if project.settings.duplicates.enabled {
        if let Some(capture) = history.as_deref_mut() {
            capture.stage = Stage::Duplicates;
        }
        let report = timings.measure("duplicates", || {
            analyze_duplicate_corpus(project, &corpus, targets)
        })?;
        if let Some(capture) = history {
            capture.duplicates(&report);
        }
        if !json {
            print_duplicates(&report, false, output.duplicate_warnings)?;
        }
        status = status.max(i32::from(report.summary.errors > 0));
        Some(report)
    } else {
        None
    };
    let json = json.then(|| {
        serde_json::json!({
            "version": 1,
            "architecture": report,
            "native": native,
            "duplicates": duplicates,
        })
    });
    Ok(CheckResult { status, json })
}

fn review_documents(mut args: Vec<String>) -> Outcome<i32> {
    let explicit = explicit_config(&mut args)?;
    let usage = "Usage: docs review <explicit-md-paths...> --reviewed YYYY-MM-DD";
    if args.first().map(String::as_str) != Some("review")
        || args
            .iter()
            .filter(|argument| *argument == "--reviewed")
            .count()
            != 1
    {
        return Err(usage.into());
    }
    args.remove(0);
    let index = args
        .iter()
        .position(|argument| argument == "--reviewed")
        .ok_or(usage)?;
    if index + 1 >= args.len() {
        return Err(usage.into());
    }
    let reviewed = args.remove(index + 1);
    args.remove(index);
    if args.is_empty() || args.iter().any(|argument| argument.starts_with('-')) {
        return Err(usage.into());
    }
    let project = discover_project(&std::env::current_dir()?, explicit.as_deref())?;
    let count = crate::features::architecture::helpers::document_content::review(
        &project, &args, &reviewed,
    )?;
    println!("Reviewed {count} documents; body text is unchanged.");
    Ok(0)
}
