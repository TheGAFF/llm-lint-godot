use crate::shared::models::Outcome;
use std::path::Path;

// Leave room below Windows' 32767 UTF-16-unit limit for the executable and flags.
const LIMIT: usize = 16_000;

fn argument_units(argument: &str) -> usize {
    // Upper bound for CRT quoting: every quote/backslash may need escaping,
    // including trailing backslashes before the closing quote. Include a space.
    argument.encode_utf16().count()
        + argument.chars().filter(|c| matches!(c, '\\' | '"')).count()
        + 3
}

pub(super) fn run(
    binary: &Path,
    paths: &[String],
    mut invoke: impl FnMut(&[String]) -> Outcome<i32>,
) -> Outcome<i32> {
    let overhead = argument_units(&binary.to_string_lossy()) + 128;
    let mut used = overhead;
    let mut start = 0;
    let mut status = 0;
    for (index, path) in paths.iter().enumerate() {
        let cost = argument_units(path);
        if overhead + cost > LIMIT {
            return Err(format!("Native lint argument exceeds command-line budget: {path}").into());
        }
        if used + cost > LIMIT {
            status = status.max(invoke(&paths[start..index])?);
            start = index;
            used = overhead;
        }
        used += cost;
    }
    if start < paths.len() {
        status = status.max(invoke(&paths[start..])?);
    }
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::{LIMIT, argument_units, run};
    use std::path::Path;

    #[test]
    fn bounded_batches_preserve_unicode_quoting_order_and_late_errors() {
        let paths: Vec<String> = (0..2000)
            .map(|i| format!("folder with spaces/日本語/😀/{i}\\\".gd"))
            .collect();
        let mut seen = Vec::new();
        let mut calls = 0;
        let result = run(Path::new("tool.exe"), &paths, |batch| {
            assert!(batch.iter().map(|p| argument_units(p)).sum::<usize>() + 139 <= LIMIT);
            calls += 1;
            seen.extend_from_slice(batch);
            Ok(i32::from(calls == 2))
        })
        .ok();
        assert_eq!(result, Some(1));
        assert!(calls > 2);
        assert_eq!(seen, paths);
        assert_eq!(argument_units("😀"), 5);
    }

    #[test]
    fn subprocess_failure_propagates_and_empty_input_does_not_spawn() {
        let mut calls = 0;
        let paths = vec!["a".repeat(8000); 3];
        assert!(
            run(Path::new("tool"), &paths, |_| {
                calls += 1;
                if calls == 2 {
                    Err("subprocess failed".into())
                } else {
                    Ok(0)
                }
            })
            .is_err()
        );
        assert_eq!(calls, 2);
        assert_eq!(
            run(Path::new("tool"), &[], |_| Err("must not spawn".into())).ok(),
            Some(0)
        );
        assert!(run(Path::new("tool"), &["x".repeat(LIMIT)], |_| Ok(0)).is_err());
    }
}
