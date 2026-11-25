use super::super::models::Release;
use crate::shared::{models::Outcome, paths::guarded_path};
use sha2::{Digest, Sha256};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) const VERSION: &str = "0.8.0";
pub(crate) const LEGACY_VERSION: &str = "0.3.1";

pub fn release_for_host() -> Outcome<Release> {
    let target = match (env::consts::OS, env::consts::ARCH) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        _ => return Err(
            "No pinned release for this platform; supply a trusted gdstyle executable and digest"
                .into(),
        ),
    };
    let releases: Vec<Release> =
        serde_json::from_str(include_str!("../../../../policy/gdstyle-release.json"))?;
    releases
        .into_iter()
        .find(|release| release.target == target)
        .ok_or_else(|| "Missing release manifest".into())
}

pub(crate) struct NativeOverride {
    path: PathBuf,
    digest: String,
}

impl NativeOverride {
    pub(crate) fn from_arguments(
        start: &Path,
        executable: Option<String>,
        digest: Option<String>,
    ) -> Outcome<Option<Self>> {
        let Some(executable) = executable else {
            return if digest.is_some() {
                Err("--gdstyle-sha256 requires --gdstyle-executable".into())
            } else {
                Ok(None)
            };
        };
        let digest = match digest {
            Some(value) if valid_digest(&value) => value.to_ascii_lowercase(),
            Some(_) => {
                return Err("--gdstyle-sha256 requires a 64-digit hexadecimal SHA-256".into());
            }
            None => release_for_host()?.executable_sha256,
        };
        Ok(Some(Self {
            path: start.join(executable),
            digest,
        }))
    }

    pub(crate) fn verified_path(&self, root: &Path) -> Outcome<PathBuf> {
        let path = fs::canonicalize(&self.path)?;
        if path.starts_with(root) {
            return Err("Choose a trusted gdstyle executable outside the project checkout".into());
        }
        verify(&path, &self.digest)?;
        Ok(path)
    }

    pub(crate) fn is_official(&self, release: &Release) -> bool {
        self.digest == release.executable_sha256
    }
}

pub(crate) fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn cache_path(root: &Path, release: &Release) -> Outcome<PathBuf> {
    let base = if let Some(path) = env::var_os("GODOT_LLM_LINT_CACHE_DIR") {
        PathBuf::from(path)
    } else if cfg!(windows) {
        env_path("LOCALAPPDATA")?.join("godot-llm-lint")
    } else if cfg!(target_os = "macos") {
        env_path("HOME")?.join("Library/Caches/godot-llm-lint")
    } else {
        env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
            .ok_or("Set XDG_CACHE_HOME, HOME, or GODOT_LLM_LINT_CACHE_DIR")?
            .join("godot-llm-lint")
    };
    if !base.is_absolute() {
        return Err("The gdstyle cache directory must be absolute".into());
    }
    let existing = base
        .ancestors()
        .find_map(|ancestor| match fs::symlink_metadata(ancestor) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            result => Some(result.map(|_| ancestor)),
        })
        .ok_or("No parent directory for gdstyle cache")??;
    let canonical = fs::canonicalize(existing)?;
    let base = guarded_path(&canonical, base.strip_prefix(existing)?)?;
    if base.starts_with(root) {
        return Err("Place the gdstyle cache outside the project checkout".into());
    }
    guarded_path(
        &base,
        &Path::new("gdstyle")
            .join(VERSION)
            .join(&release.target)
            .join(&release.executable),
    )
}

fn env_path(name: &str) -> Outcome<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| format!("Set {name} or GODOT_LLM_LINT_CACHE_DIR").into())
}

pub(crate) fn verify(binary: &Path, expected: &str) -> Outcome<()> {
    let metadata = fs::symlink_metadata(binary)?;
    if !metadata.is_file() || crate::shared::paths::is_link(&metadata) {
        return Err("Expected a regular gdstyle executable".into());
    }
    let hash = format!("{:x}", Sha256::digest(fs::read(binary)?));
    if hash != expected {
        return Err("gdstyle executable checksum mismatch".into());
    }
    let version = Command::new(binary)
        .current_dir(
            binary
                .parent()
                .ok_or("Missing gdstyle executable directory")?,
        )
        .arg("--version")
        .output()?;
    if !version.status.success()
        || String::from_utf8(version.stdout)?.trim() != format!("gdstyle {VERSION}")
    {
        return Err(format!("Expected gdstyle {VERSION}").into());
    }
    Ok(())
}

pub(crate) fn system_tool(name: &str) -> Outcome<PathBuf> {
    let search = env::var_os("PATH").ok_or("Set PATH to locate the required system tools")?;
    env::split_paths(&search)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
        .ok_or_else(|| format!("Required executable {name} is missing from PATH").into())
}
