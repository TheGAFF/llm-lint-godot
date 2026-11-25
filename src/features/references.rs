mod collect;
mod io;
mod report;
mod serialized;

use crate::features::{
    architecture::helpers::{identity, parsed_corpus},
    configuration::{
        helpers::{discover_settings, safe_path, target_paths},
        models::Project,
    },
    parsing::parsers::parse_text,
};
use crate::shared::models::{Corpus, Outcome};
use report::{Report, Subject};
use std::path::Path;

pub(crate) const USAGE: &str = "references <path-or-uid> [--json] [--config FILE]";

pub(crate) fn run(args: &[String], config: Option<&Path>) -> Outcome<i32> {
    if matches!(args, [flag] if flag == "--help" || flag == "-h") {
        println!("Usage: godot-llm-lint {USAGE}");
        return Ok(0);
    }
    let (query, json) = match args {
        [query] => (query, false),
        [query, flag] if flag == "--json" => (query, true),
        [flag, query] if flag == "--json" => (query, true),
        _ => return Err(format!("Usage: godot-llm-lint {USAGE}").into()),
    };
    if query.is_empty() || query.starts_with('-') {
        return Err(format!("Usage: godot-llm-lint {USAGE}").into());
    }
    let project = discover_settings(&std::env::current_dir()?, config)?;
    let (corpus, trees) = parsed_corpus(&project)?;
    let subject = subject(&project, &corpus, query)?;
    let mut report = Report::new(subject);
    collect::find(&project, &corpus, &trees, &mut report)?;
    report.sort();
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        report.print();
    }
    Ok(0)
}

fn subject(project: &Project, corpus: &Corpus, query: &str) -> Outcome<Subject> {
    let path = if query.starts_with("uid://") {
        corpus
            .uids
            .get(query)
            .cloned()
            .ok_or_else(|| format!("Unknown UID: {query}"))?
    } else if let Some(path) = query.strip_prefix("res://") {
        safe_path(path)?;
        path.into()
    } else {
        target_paths(project, &[query.replace('\\', "/")])?.remove(0)
    };
    if !corpus.paths.contains(&path) || !project.root.join(&path).is_file() {
        return Err(format!("Not an indexed project file: {query}").into());
    }
    let mut source = parse_text(&path, String::new());
    identity(project, &mut source);
    let uids = corpus
        .uids
        .iter()
        .filter(|(_, file)| **file == path)
        .map(|(uid, _)| uid.clone())
        .collect();
    Ok(Subject {
        path,
        owner: (!source.owner.is_empty()).then_some(source.owner),
        uids,
    })
}
