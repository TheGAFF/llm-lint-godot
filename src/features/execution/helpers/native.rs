use super::batches;
use crate::{
    features::{
        configuration::{
            helpers::native::{self, NativeOverride},
            models::Project,
            rules,
        },
        execution::models::NativeFinding,
    },
    shared::models::Outcome,
};
use std::process::Command;

pub fn native_checks(project: &Project, paths: &[String], formatting: bool) -> Outcome<i32> {
    native_checks_with(project, paths, formatting, false, None, &mut |_| {})
}

pub(crate) fn native_checks_with(
    project: &Project,
    paths: &[String],
    formatting: bool,
    json: bool,
    selected: Option<&NativeOverride>,
    observe: &mut dyn FnMut(NativeFinding<'_>),
) -> Outcome<i32> {
    let binary = if let Some(selected) = selected {
        selected.verified_path(&project.root)?
    } else {
        let tool = project
            .settings
            .gdstyle
            .as_ref()
            .ok_or("Run godot-llm-lint init to configure the pinned gdstyle")?;
        let release = native::release_for_host()?;
        if !tool.auto && !tool.sha256.eq_ignore_ascii_case(&release.executable_sha256) {
            return Err("Project configuration cannot authorize custom gdstyle executables; supply --gdstyle-executable and --gdstyle-sha256".into());
        }
        let binary = native::cache_path(&project.root, &release)?;
        native::verify(&binary, &release.executable_sha256).map_err(|error| {
            format!("Run godot-llm-lint init to provision the pinned gdstyle: {error}")
        })?;
        binary
    };
    if paths.is_empty() {
        return Ok(0);
    }
    let mut status = batches::run(&binary, paths, |batch| {
        invoke(project, &binary, batch, false, json, observe)
    })?;
    if formatting {
        status = status.max(formatting_checks(project, &binary, paths, json, observe)?);
    }
    Ok(status)
}

fn invoke(
    project: &Project,
    binary: &std::path::Path,
    paths: &[String],
    formatting: bool,
    json: bool,
    observe: &mut dyn FnMut(NativeFinding<'_>),
) -> Outcome<i32> {
    let mut command = Command::new(binary);
    command.current_dir(&project.root);
    command.arg(if formatting { "fmt" } else { "check" });
    command.args(["--config", "gdstyle.toml", "--no-color"]);
    if formatting {
        command.arg("--check");
    } else {
        command.args(["--format", "json"]);
    }
    command.arg("--").args(paths);
    let result = command.output()?;
    if !formatting {
        if !matches!(result.status.code(), Some(0 | 1)) {
            return Err(format!(
                "gdstyle tool failure: {}",
                String::from_utf8_lossy(&result.stderr)
            )
            .into());
        }
        let diagnostics: Vec<serde_json::Value> = serde_json::from_slice(&result.stdout)?;
        let mut errors = 0;
        let catalog = rules::catalog()?;
        for diagnostic in diagnostics {
            let id = format!(
                "gdstyle/{}",
                diagnostic["rule"].as_str().ok_or("Missing native rule")?
            );
            let definition = catalog
                .iter()
                .find(|entry| entry["id"] == id)
                .ok_or_else(|| format!("Unknown native rule: {id}"))?;
            let file = diagnostic["file"]
                .as_str()
                .ok_or("Missing native diagnostic path")?;
            let path = std::path::Path::new(file);
            let filename = path
                .strip_prefix(&project.root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let level = rules::level(
                &project.settings.rules,
                &project.settings.overrides,
                &id,
                &filename,
                definition["default"].as_str().unwrap_or("error"),
            );
            if level == "off" {
                continue;
            }
            if level == "error" {
                errors += 1;
            }
            observe(NativeFinding {
                rule: &id,
                level,
                path: &filename,
                line: diagnostic["span"]["line"].as_i64(),
                column: diagnostic["span"]["column"].as_i64(),
            });
            eprintln!(
                "{}:{}:{} {} {}: {}",
                filename,
                diagnostic["span"]["line"],
                diagnostic["span"]["column"],
                level,
                id,
                diagnostic["message"]
                    .as_str()
                    .unwrap_or("Native diagnostic")
            );
        }
        return Ok(i32::from(errors > 0));
    }
    if json {
        eprint!("{}", String::from_utf8_lossy(&result.stdout));
    } else {
        print!("{}", String::from_utf8_lossy(&result.stdout));
    }
    eprint!("{}", String::from_utf8_lossy(&result.stderr));
    match result.status.code() {
        Some(0) => Ok(0),
        Some(1) => Ok(1),
        _ => Err("gdstyle failed without a lint result".into()),
    }
}

fn formatting_checks(
    project: &Project,
    binary: &std::path::Path,
    paths: &[String],
    json: bool,
    observe: &mut dyn FnMut(NativeFinding<'_>),
) -> Outcome<i32> {
    let mut status = 0;
    for path in paths {
        let level = rules::level(
            &project.settings.rules,
            &project.settings.overrides,
            "formatting/check",
            path,
            "error",
        );
        if level == "off" {
            continue;
        }
        let found = invoke(
            project,
            binary,
            std::slice::from_ref(path),
            true,
            json,
            observe,
        )?;
        if found == 1 {
            observe(NativeFinding {
                rule: "formatting/check",
                level,
                path,
                line: None,
                column: None,
            });
        }
        if level == "error" {
            status = status.max(found);
        }
    }
    Ok(status)
}
