use super::super::models::{RecordOptions, ReportOptions};
use crate::shared::models::Outcome;

impl RecordOptions {
    pub(crate) fn parse(args: &mut Vec<String>) -> Outcome<Option<Self>> {
        let count = args
            .iter()
            .filter(|argument| *argument == "--record")
            .count();
        let model = argument(args, "--model")?;
        let baseline = argument(args, "--baseline-run")?;
        if count == 0 {
            if model.is_some() || baseline.is_some() {
                return Err("--model and --baseline-run on check require --record".into());
            }
            return Ok(None);
        }
        if count > 1 {
            return Err("Repeated --record".into());
        }
        args.retain(|argument| argument != "--record");
        if !args.is_empty() {
            return Err("History recording requires a full-project check without paths".into());
        }
        Ok(Some(Self {
            model: model_label(model.as_deref().ok_or("--record requires --model")?)?,
            baseline: baseline.as_deref().map(run_id).transpose()?,
        }))
    }
}

impl ReportOptions {
    pub(crate) fn parse(mut args: Vec<String>) -> Outcome<Self> {
        if args.first().is_none_or(|argument| argument != "report") {
            return Err("Usage: history report [--model LABEL] [--since YYYY-MM-DD] [--run ID] [--html FILE]".into());
        }
        args.remove(0);
        let options = Self {
            model: argument(&mut args, "--model")?
                .as_deref()
                .map(model_label)
                .transpose()?,
            since: argument(&mut args, "--since")?,
            run: argument(&mut args, "--run")?
                .as_deref()
                .map(run_id)
                .transpose()?,
            html: argument(&mut args, "--html")?.map(Into::into),
        };
        if !args.is_empty() {
            return Err("Unexpected argument to history report".into());
        }
        Ok(options)
    }
}

fn argument(args: &mut Vec<String>, flag: &str) -> Outcome<Option<String>> {
    let mut indices = args.iter().enumerate().filter(|(_, item)| *item == flag);
    let Some((index, _)) = indices.next() else {
        return Ok(None);
    };
    if indices.next().is_some() {
        return Err(format!("Repeated {flag}").into());
    }
    if args
        .get(index + 1)
        .is_none_or(|value| value.starts_with("--"))
    {
        return Err(format!("{flag} requires a value").into());
    }
    let value = args.remove(index + 1);
    args.remove(index);
    Ok(Some(value))
}

fn model_label(value: &str) -> Outcome<String> {
    let label = value.trim();
    if label.is_empty() || value.chars().any(char::is_control) {
        return Err("--model requires a nonempty label without control characters".into());
    }
    Ok(label.into())
}

fn run_id(value: &str) -> Outcome<i64> {
    let id = value.parse::<i64>().ok().filter(|id| *id > 0);
    id.ok_or_else(|| "Run ID must be a positive integer".into())
}
