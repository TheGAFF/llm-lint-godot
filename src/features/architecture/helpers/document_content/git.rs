use super::{Document, markdown, records, sources};
use crate::{
    features::{
        configuration::{
            helpers::{inside, native::system_tool},
            models::Project,
        },
        parsing::parsers::parse_text,
    },
    shared::models::Outcome,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

pub(crate) struct GitBaseline {
    pub(crate) commit: String,
    root: PathBuf,
    prefix: String,
    tree: Option<GitTree>,
    batch: Option<BlobReader>,
    hashes: BTreeMap<String, String>,
}

impl GitBaseline {
    pub(crate) fn resolve(project: &Project, reference: &str) -> Outcome<Self> {
        let name = format!("{reference}^{{commit}}");
        let resolve = || -> Outcome<(String, String)> {
            let commit = output(
                &project.root,
                &["rev-parse", "--verify", "--end-of-options", &name],
            )?;
            let prefix = output(&project.root, &["rev-parse", "--show-prefix"])?;
            Ok((
                String::from_utf8(commit)?
                    .trim_end_matches(['\n', '\r'])
                    .into(),
                String::from_utf8(prefix)?
                    .trim_end_matches(['\n', '\r'])
                    .into(),
            ))
        };
        let (commit, prefix) =
            resolve().map_err(|error| format!("Invalid --doc-baseline {reference:?}: {error}"))?;
        Ok(Self {
            commit,
            root: project.root.clone(),
            prefix,
            tree: None,
            batch: None,
            hashes: BTreeMap::new(),
        })
    }

    pub(super) fn snapshot(&mut self, project: &Project, path: &str) -> Outcome<sources::Snapshot> {
        self.prepare(project)?;
        let bytes = self.read(path)?;
        let text = String::from_utf8(bytes)?;
        let source = parse_text(path, text);
        let document = Document::parse(&source).ok_or("No document at the baseline path")?;
        let tree = self.tree.as_ref().ok_or("Git baseline tree unavailable")?;
        let scope = sources::scope(
            &document,
            &markdown::declarations(&document),
            &tree.paths,
            |_| true,
            &mut Vec::new(),
        )
        .ok_or("Invalid Key Files declarations at the Git baseline")?;
        if !scope.missing_lines.is_empty() {
            return Err("Some declared sources are absent at the Git baseline".into());
        }
        scope.snapshot(|source| self.hash(source))
    }

    fn prepare(&mut self, project: &Project) -> Outcome<()> {
        if self.tree.is_none() {
            let listed = output(&self.root, &["ls-tree", "-rz", "--full-tree", &self.commit])?;
            self.tree = Some(GitTree::new(&listed, &self.prefix, project));
        }
        if self.batch.is_none() {
            self.batch = Some(BlobReader::start(&self.root)?);
        }
        Ok(())
    }

    fn read(&mut self, path: &str) -> Outcome<Vec<u8>> {
        let oid = self
            .tree
            .as_ref()
            .and_then(|tree| tree.blobs.get(path))
            .ok_or_else(|| format!("Missing path at the Git baseline: {path}"))?;
        self.batch
            .as_mut()
            .ok_or("Git baseline reader unavailable")?
            .read(oid)
    }

    fn hash(&mut self, path: &str) -> Outcome<String> {
        if let Some(hash) = self.hashes.get(path) {
            return Ok(hash.clone());
        }
        let hash = sources::hash_bytes(&self.read(path)?, path);
        self.hashes.insert(path.into(), hash.clone());
        Ok(hash)
    }
}

struct GitTree {
    paths: BTreeSet<String>,
    blobs: BTreeMap<String, String>,
}

impl GitTree {
    fn new(listed: &[u8], prefix: &str, project: &Project) -> Self {
        let blobs: BTreeMap<_, _> = listed
            .split(|byte| *byte == 0)
            .filter_map(|row| blob_entry(row, prefix))
            .filter(|(path, _)| visible(project, path))
            .map(|(path, id)| (path.to_owned(), id.to_owned()))
            .collect();
        let paths = blobs.keys().cloned().collect();
        Self { paths, blobs }
    }
}

fn blob_entry<'a>(row: &'a [u8], prefix: &str) -> Option<(&'a str, &'a str)> {
    let (header, path) = std::str::from_utf8(row).ok()?.split_once('\t')?;
    let (mode, oid) = header.split_once(" blob ")?;
    matches!(mode, "100644" | "100755").then_some((path.strip_prefix(prefix)?, oid))
}

fn visible(project: &Project, path: &str) -> bool {
    !inside(path, records::DIRECTORY)
        && !path
            .split('/')
            .any(|part| super::super::sources::PRUNED_NAMES.contains(&part))
        && !project
            .settings
            .excluded
            .iter()
            .any(|entry| entry.skip_discovery && inside(path, &entry.path))
}

fn command(root: &Path) -> Outcome<Command> {
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let mut git = Command::new(system_tool(name)?);
    git.current_dir(root).env("GIT_NO_LAZY_FETCH", "1");
    Ok(git)
}

fn output(root: &Path, args: &[&str]) -> Outcome<Vec<u8>> {
    let result = command(root)?.args(args).output()?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr)
            .trim()
            .to_owned()
            .into());
    }
    Ok(result.stdout)
}

struct BlobReader {
    process: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl BlobReader {
    fn start(root: &Path) -> Outcome<Self> {
        let mut process = command(root)?
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = process.stdin.take().ok_or("Missing Git blob input pipe")?;
        let output = BufReader::new(
            process
                .stdout
                .take()
                .ok_or("Missing Git blob output pipe")?,
        );
        Ok(Self {
            process,
            input,
            output,
        })
    }

    fn read(&mut self, oid: &str) -> Outcome<Vec<u8>> {
        writeln!(self.input, "{oid}")?;
        self.input.flush()?;
        let mut line = String::new();
        self.output.read_line(&mut line)?;
        let prefix = format!("{oid} blob ");
        let size: usize = line
            .strip_prefix(&prefix)
            .ok_or("Missing or invalid Git baseline blob")?
            .trim_end()
            .parse()?;
        let mut bytes = vec![0; size];
        self.output.read_exact(&mut bytes)?;
        let mut terminator = [0];
        self.output.read_exact(&mut terminator)?;
        if terminator != *b"\n" {
            return Err("Unexpected Git baseline blob terminator".into());
        }
        Ok(bytes)
    }
}

impl Drop for BlobReader {
    fn drop(&mut self) {
        let _killed = self.process.kill();
        let _reaped = self.process.wait();
    }
}
