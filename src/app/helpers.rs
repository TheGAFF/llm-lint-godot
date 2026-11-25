use crate::{
    features::{
        configuration::helpers::project_config,
        history::models::{Report, ReportOptions},
    },
    shared::models::Outcome,
};

pub(super) fn history_report(mut args: Vec<String>) -> Outcome<i32> {
    let explicit = explicit_config(&mut args)?;
    let options = ReportOptions::parse(args)?;
    let (root, _) = project_config(&std::env::current_dir()?, explicit.as_deref())?;
    let report = Report::load(&root, &options)?;
    if let Some(path) = options.html {
        report.export(&path)?;
        println!("Wrote lint history to {}", path.display());
    } else {
        report.print()?;
    }
    Ok(0)
}

pub(super) fn explicit_config(args: &mut Vec<String>) -> Outcome<Option<std::path::PathBuf>> {
    let explicit = if let Some(index) = args.iter().position(|argument| argument == "--config") {
        if index + 1 >= args.len() {
            return Err("--config requires a filename".into());
        }
        let filename = args.remove(index + 1);
        args.remove(index);
        Some(std::path::PathBuf::from(filename))
    } else {
        None
    };
    Ok(explicit)
}
