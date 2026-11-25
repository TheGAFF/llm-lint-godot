#![cfg(test)]
mod support;

use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use support::{project, write};

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_godot-llm-lint"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn native_authority_comes_from_the_caller() {
    let root = tempfile::tempdir().unwrap();
    let trusted = std::env::var_os("GDSTYLE_TEST_BINARY").expect("Provide the pinned gdstyle");
    let trusted = trusted.to_str().unwrap();
    let trusted_digest = format!("{:x}", Sha256::digest(fs::read(trusted).unwrap()));
    let bytes = "placeholder executable";
    let checkout_digest = format!("{:x}", Sha256::digest(bytes));
    let policy = format!(
        "[gdstyle]\nversion='0.8.0'\nexecutable='checkout-tool'\nsha256='{checkout_digest}'\n"
    );
    project(root.path(), &policy);
    write(
        root.path(),
        "src/shared/queries/value_query.gd",
        "extends RefCounted\n\n\nstatic func get_value() -> int:\n\treturn 1\n",
    );
    write(root.path(), "checkout-tool", bytes);

    let rejected = run(root.path(), &["check", "--json"]);
    assert_eq!(rejected.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&rejected.stdout).contains("cannot authorize custom gdstyle"));
    let local = run(
        root.path(),
        &[
            "check",
            "--gdstyle-executable",
            "checkout-tool",
            "--gdstyle-sha256",
            &checkout_digest,
        ],
    );
    assert_eq!(local.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&local.stderr).contains("outside the project checkout"));

    let rejected = run(
        root.path(),
        &[
            "check",
            "--gdstyle-executable",
            trusted,
            "--gdstyle-sha256",
            &checkout_digest,
        ],
    );
    assert_eq!(rejected.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("executable checksum mismatch"));
    let allowed = run(
        root.path(),
        &[
            "check",
            "--gdstyle-executable",
            trusted,
            "--gdstyle-sha256",
            &trusted_digest,
        ],
    );
    assert!(allowed.status.success(), "{allowed:?}");
}

#[test]
fn history_and_corpus_reject_linked_directories() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    project(root.path(), "[gdstyle]\nauto=true\nversion='0.8.0'\n");
    write(external.path(), "external.gd", "extends Node");
    link_dir(external.path(), &root.path().join(".tools"));

    let rejected = run(
        root.path(),
        &["check", "--record", "--model", "Release test"],
    );
    assert_eq!(rejected.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("reparse point"));
    assert!(!external.path().join("godot-llm-lint").exists());

    link_dir(external.path(), &root.path().join("src/linked"));
    let rejected = run(root.path(), &["architecture", "--json"]);
    assert_eq!(rejected.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&rejected.stdout).contains("Symlink inside project corpus"));
}

#[cfg(unix)]
fn link_dir(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(windows)]
fn link_dir(target: &Path, link: &Path) {
    let output = Command::new("pwsh")
        .args(["-NoProfile", "-NonInteractive", "-Command", "New-Item -ItemType Junction -Path $env:RELEASE_TEST_LINK -Target $env:RELEASE_TEST_TARGET -ErrorAction Stop"])
        .env("RELEASE_TEST_LINK", link)
        .env("RELEASE_TEST_TARGET", target)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}
