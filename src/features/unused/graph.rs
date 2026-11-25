use super::{
    ALL_KINDS, Finding, Gap, Kind, Options, inputs,
    release::{self, Advisory, Retention, RetentionKind},
};
use crate::{
    features::configuration::{helpers::inside, models::Project},
    shared::models::{Corpus, Outcome, Reference, Source},
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    ops::Bound,
};

pub(super) struct Graph<'a> {
    project: &'a Project,
    corpus: &'a Corpus,
    files: BTreeSet<&'a str>,
    sources: BTreeMap<&'a str, &'a Source>,
    classes: BTreeMap<&'a str, Vec<&'a str>>,
    release_assets: release::Index<'a>,
    runtime_release_assets: BTreeSet<&'a str>,
    pub(super) reachable: BTreeSet<&'a str>,
    pub(super) roots: BTreeSet<&'a str>,
    pub(super) gaps: Vec<Gap>,
    pub(super) file_count: usize,
}

impl<'a> Graph<'a> {
    pub(super) fn build(
        project: &'a Project,
        corpus: &'a Corpus,
        config_name: &str,
    ) -> Outcome<Self> {
        let files: BTreeSet<_> = corpus
            .paths
            .iter()
            .map(String::as_str)
            .filter(|path| project.root.join(path).is_file())
            .collect();
        let sources: BTreeMap<_, _> = corpus
            .sources
            .iter()
            .chain(&corpus.excluded_sources)
            .map(|source| (source.path.as_str(), source))
            .collect();
        let mut classes: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (&path, source) in &sources {
            if let Some(name) = &source.class_name {
                classes.entry(name).or_default().push(path);
            }
        }
        let file_count = files.len();
        let release_assets = release::load(project, &files)?;
        let mut graph = Self {
            project,
            corpus,
            files,
            sources,
            classes,
            release_assets,
            runtime_release_assets: BTreeSet::new(),
            file_count,
            reachable: BTreeSet::new(),
            roots: BTreeSet::new(),
            gaps: Vec::new(),
        };
        let runtime = graph.select_roots(config_name)?;
        graph.walk(
            runtime.iter().copied().collect(),
            RetentionKind::RuntimeReference,
        )?;
        graph.runtime_release_assets.extend(
            graph
                .release_assets
                .keys()
                .filter(|path| graph.reachable.contains(*path))
                .copied(),
        );
        let pending = graph
            .roots
            .iter()
            .chain(graph.release_assets.keys())
            .copied()
            .collect();
        graph.walk(pending, RetentionKind::ProjectReference)?;
        if graph.roots.is_empty() {
            graph.gaps.push(Gap {
                path: "project.godot".into(),
                line: 1,
                message:
                    "No entry points resolved. Check project settings or configure entrypoints."
                        .into(),
                kinds: ALL_KINDS,
            });
        }
        Ok(graph)
    }

    fn walk(&mut self, mut pending: VecDeque<&'a str>, kind: RetentionKind) -> Outcome<()> {
        while let Some(path) = pending.pop_front() {
            if !self.reachable.insert(path) {
                continue;
            }
            let edges = self.dependencies(path)?;
            for &target in edges.iter().filter(|&&target| target != path) {
                record_reference(&mut self.release_assets, target, path, kind, None);
            }
            pending.extend(edges);
        }
        Ok(())
    }

    fn select_roots(&mut self, config_name: &str) -> Outcome<BTreeSet<&'a str>> {
        let project = self.project;
        let mut roots = BTreeSet::new();
        let mut runtime = BTreeSet::new();
        if let Some(source) = self.sources.get("project.godot") {
            self.project_roots(&source.text, &mut roots, &mut runtime);
        }
        self.config_roots(&mut roots, config_name);
        let presets = self
            .files
            .contains("export_presets.cfg")
            .then(|| inputs::read(&project.root, "export_presets.cfg"))
            .transpose()?;
        let presets = presets.as_deref().map_or_else(Vec::new, inputs::presets);
        for &path in &self.files {
            if inputs::ancillary(path) {
                continue;
            }
            let tool = inputs::tool_source(path)
                && project
                    .settings
                    .tooling_roots
                    .iter()
                    .any(|root| inside(path, root));
            if presets.iter().any(|preset| preset.includes(path)) {
                roots.insert(path);
                record_reference(
                    &mut self.release_assets,
                    path,
                    "export_presets.cfg",
                    RetentionKind::ProjectReference,
                    Some("Explicit export selection"),
                );
            }
            if tool || self.is_test(path) {
                roots.insert(path);
            }
            if matches!(inputs::extension(path), "gdextension" | "csproj") {
                runtime.insert(path);
            }
        }
        roots.extend(&runtime);
        self.roots = roots;
        Ok(runtime)
    }

    fn config_roots(&mut self, roots: &mut BTreeSet<&'a str>, config_name: &str) {
        let settings = &self.project.settings;
        let entries = settings
            .entrypoints
            .iter()
            .map(|entry| (&entry.path, Some(entry.reason.as_str())))
            .chain(
                settings
                    .unused
                    .keep
                    .iter()
                    .map(|entry| (&entry.path, Some(entry.reason.as_str()))),
            )
            .chain(
                settings
                    .unused
                    .reference_files
                    .iter()
                    .chain(&settings.unused.release_exclusion_files)
                    .map(|path| (path, None)),
            );
        for (path, reason) in entries {
            let mut expanded = BTreeSet::new();
            self.expand(path, &mut expanded);
            for target in expanded {
                roots.insert(target);
                record_reference(
                    &mut self.release_assets,
                    target,
                    config_name,
                    RetentionKind::ProjectReference,
                    reason,
                );
            }
        }
    }

    fn project_roots(
        &mut self,
        text: &str,
        roots: &mut BTreeSet<&'a str>,
        runtime: &mut BTreeSet<&'a str>,
    ) {
        let values = inputs::settings(text).flat_map(|(line, section, _, value)| {
            inputs::strings(value).map(move |value| (line, section, value.trim_start_matches('*')))
        });
        for (line, section, raw) in values {
            if !is_resource(raw) && section != "autoload" {
                continue;
            }
            let selected = if section == "editor_plugins" {
                &mut *roots
            } else {
                &mut *runtime
            };
            self.extend_reference(selected, "project.godot", line, raw, "", is_resource(raw));
        }
        for &target in runtime.iter() {
            record_reference(
                &mut self.release_assets,
                target,
                "project.godot",
                RetentionKind::RuntimeReference,
                None,
            );
        }
    }

    fn is_test(&self, path: &str) -> bool {
        let Some(source) = self.sources.get(path) else {
            return false;
        };
        let name = path.rsplit('/').next().unwrap_or(path);
        name.ends_with(".gd")
            && (source.testing || path.split('/').any(|part| matches!(part, "test" | "tests")))
            && (name.starts_with("test_")
                || name.ends_with("_test_harness.gd")
                || source.base.as_deref() == Some("GutTest"))
    }

    fn dependencies(&mut self, path: &'a str) -> Outcome<BTreeSet<&'a str>> {
        let mut edges = BTreeSet::new();
        let extension = inputs::extension(path);
        if let Some(source) = self.sources.get(path) {
            self.source_edges(source, extension, &mut edges)?;
        }
        if self.files.contains(format!("{path}.import").as_str()) {
            self.import_edges(path, &mut edges)?;
        }
        if is_opaque(extension) {
            self.gaps.push(Gap {
                path: path.into(),
                line: 1,
                message: format!(
                    "Reachable .{extension} may contain references outside the supported formats."
                ),
                kinds: ALL_KINDS,
            });
        } else {
            self.text_edges(path, extension, &mut edges)?;
        }
        Ok(edges)
    }

    fn source_edges(
        &mut self,
        source: &'a Source,
        extension: &str,
        edges: &mut BTreeSet<&'a str>,
    ) -> Outcome<()> {
        let path = source.path.as_str();
        if extension == "gd" {
            self.script_edges(source, edges)?;
            return Ok(());
        }
        if path.ends_with("/plugin.cfg") || path == "plugin.cfg" {
            let script = inputs::settings(&source.text)
                .find(|(_, section, key, _)| *section == "plugin" && *key == "script");
            if let Some((line, _, _, value)) = script
                && let Some(target) = inputs::strings(value).next()
            {
                self.extend_reference(edges, path, line, target, inputs::parent(path), true);
            }
            return Ok(());
        }
        if matches!(extension, "tscn" | "tres" | "escn") {
            self.external_resources(edges, path, &source.text);
        }
        for reference in &source.references {
            if reference.item.starts_with("[ext_resource")
                || (reference.item.starts_with("[gd_") && reference.target.starts_with("uid://"))
            {
                continue;
            }
            if !reference.dynamic {
                self.extend_reference(
                    edges,
                    path,
                    reference.line,
                    &reference.target,
                    inputs::parent(path),
                    reference.required,
                );
            }
        }
        Ok(())
    }

    fn external_resources(&mut self, edges: &mut BTreeSet<&'a str>, path: &str, text: &str) {
        let headers = text
            .lines()
            .enumerate()
            .filter(|(_, line)| line.trim_start().starts_with("[ext_resource "));
        for (line, header) in headers {
            self.external_resource(edges, path, line + 1, header);
        }
    }

    fn external_resource(
        &mut self,
        edges: &mut BTreeSet<&'a str>,
        path: &str,
        line: usize,
        header: &str,
    ) {
        let uid = inputs::attribute(header, "uid");
        let saved = inputs::attribute(header, "path");
        let uid_path = uid
            .and_then(|uid| self.corpus.uids.get(uid))
            .map(String::as_str);
        let saved_path = saved.and_then(|value| self.resolve(inputs::parent(path), value));
        if let Some(uid_path) = uid_path {
            self.expand(uid_path, edges);
        }
        if let Some(saved_path) = &saved_path
            && self.expand(saved_path, edges)
        {
            return;
        }
        if uid_path.is_none() {
            let target = saved.or(uid).unwrap_or("<missing path>");
            self.missing(path, line, target);
        }
    }

    fn import_edges(&mut self, path: &str, edges: &mut BTreeSet<&'a str>) -> Outcome<()> {
        let sidecar = format!("{path}.import");
        let text = inputs::read(&self.project.root, &sidecar)?;
        for (line, section, _, value) in inputs::settings(&text) {
            if section != "params" {
                continue;
            }
            for target in inputs::strings(value).filter(|value| value.starts_with("res://")) {
                self.extend_reference(edges, &sidecar, line, target, "", true);
            }
        }
        Ok(())
    }

    fn script_edges(&mut self, source: &'a Source, edges: &mut BTreeSet<&'a str>) -> Outcome<()> {
        let path = source.path.as_str();
        for diagnostic in source
            .diagnostics
            .iter()
            .filter(|item| item.rule == "architecture/syntax")
        {
            self.gaps.push(Gap {
                path: path.into(),
                line: diagnostic.line,
                message: "GDScript contains syntax the parser could not analyze.".into(),
                kinds: ALL_KINDS,
            });
        }
        for (name, _, _) in &source.class_uses {
            if let Some(scripts) = self.classes.get(name.as_str()) {
                edges.extend(scripts);
            }
        }
        for reference in &source.references {
            if reference.dynamic {
                self.dynamic(source, reference, edges);
            } else {
                self.extend_reference(
                    edges,
                    path,
                    reference.line,
                    &reference.target,
                    inputs::parent(path),
                    reference.required,
                );
            }
        }
        for (line, value) in inputs::script_io(&source.text)? {
            self.io_reference(source, line, &value, edges);
        }
        Ok(())
    }

    fn io_reference(
        &mut self,
        source: &Source,
        line: usize,
        target: &str,
        edges: &mut BTreeSet<&'a str>,
    ) {
        let path = source.path.as_str();
        if target.starts_with("user://") || target.starts_with("res://.tools/") {
            return;
        }
        if self
            .resolve("", target)
            .is_some_and(|resolved| self.add_file(&resolved, edges))
        {
            return;
        }
        let item = source
            .functions
            .iter()
            .filter(|function| {
                function.line <= line
                    && function
                        .tokens
                        .last()
                        .is_some_and(|token| token.end_line >= line)
            })
            .max_by_key(|function| function.line)
            .map_or("<script>", |function| function.name.as_str());
        if !self.expand_load(path, item, edges) {
            self.missing(path, line, target);
        }
    }

    fn dynamic(&mut self, source: &Source, reference: &Reference, edges: &mut BTreeSet<&'a str>) {
        let path = &source.path;
        let target = &reference.target;
        if self.expand_load(path, &reference.item, edges) || target.starts_with("user://") {
            return;
        }
        if self
            .resolve(inputs::parent(path), target)
            .is_some_and(|path| self.add_file(&path, edges))
        {
            return;
        }
        self.missing(path, reference.line, target);
    }

    fn expand_load(&self, path: &str, item: &str, edges: &mut BTreeSet<&'a str>) -> bool {
        let Some(decl) = self
            .project
            .settings
            .dynamic_loads
            .iter()
            .find(|decl| decl.path == path && decl.item == item)
        else {
            return false;
        };
        let mut complete = true;
        for root in &decl.roots {
            if let Some(root) = root.strip_prefix("res://") {
                complete &= self.expand_matching(root, edges, |path| decl.matches_extension(path));
            }
        }
        complete
    }

    fn text_edges(
        &mut self,
        path: &str,
        extension: &str,
        edges: &mut BTreeSet<&'a str>,
    ) -> Outcome<()> {
        let configured = self
            .project
            .settings
            .unused
            .reference_files
            .iter()
            .any(|input| input == path);
        match extension {
            "gdshader" | "gdshaderinc" => self.shader_edges(path, edges)?,
            "json" | "gltf" => {
                let text = inputs::read(&self.project.root, path)?;
                let manifest = self
                    .project
                    .settings
                    .unused
                    .release_exclusion_files
                    .iter()
                    .any(|input| input == path);
                match inputs::json_paths(&text, manifest) {
                    Ok(values) => self.extra_edges(path, values.iter().map(String::as_str), edges),
                    Err(error) => self.gaps.push(inputs::file_gap(
                        path,
                        error.line(),
                        format!("Could not parse .{extension} references: {error}"),
                    )),
                }
            }
            "escn" => {
                if !self.sources.contains_key(path) {
                    self.external_resources(edges, path, &inputs::read(&self.project.root, path)?);
                }
            }
            _ if inputs::tool_source(path) || configured => {
                let text = inputs::read(&self.project.root, path)?;
                let lines = text
                    .lines()
                    .filter(|line| !line.trim_start().starts_with(['#', ';']));
                self.extra_edges(path, lines.flat_map(inputs::strings), edges);
            }
            _ => (),
        }
        Ok(())
    }

    fn shader_edges(&mut self, path: &str, edges: &mut BTreeSet<&'a str>) -> Outcome<()> {
        let text = inputs::read(&self.project.root, path)?;
        for (line, directive) in text.lines().enumerate() {
            if let Some(include) = directive
                .trim_start()
                .strip_prefix('#')
                .and_then(|line| line.trim_start().strip_prefix("include"))
                .and_then(|line| inputs::strings(line).next())
            {
                self.extend_reference(edges, path, line + 1, include, inputs::parent(path), true);
            }
        }
        Ok(())
    }

    fn extra_edges<'b>(
        &self,
        path: &str,
        values: impl Iterator<Item = &'b str>,
        edges: &mut BTreeSet<&'a str>,
    ) {
        for value in values.filter(|value| value.contains('/') || value.contains('.')) {
            let value = value.replace('\\', "/");
            let candidates = ["", inputs::parent(path)]
                .into_iter()
                .filter_map(|base| self.resolve(base, &value));
            for local in candidates {
                self.add_file(&local, edges);
            }
        }
    }

    fn resolve(&self, base: &str, raw: &str) -> Option<String> {
        crate::features::parsing::helpers::resource_paths::resolve(&self.corpus.uids, base, raw)
    }

    fn expand(&self, path: &str, edges: &mut BTreeSet<&'a str>) -> bool {
        self.expand_matching(path, edges, |_| true)
    }

    fn expand_matching(
        &self,
        path: &str,
        edges: &mut BTreeSet<&'a str>,
        allows: impl Fn(&str) -> bool,
    ) -> bool {
        if self.files.contains(path) {
            if allows(path) {
                self.add_file(path, edges);
            }
            return true;
        } else if path.is_empty() {
            edges.extend(
                self.files
                    .iter()
                    .copied()
                    .filter(|entry| !inputs::ancillary(entry) && allows(entry)),
            );
        } else if self.corpus.paths.contains(path) {
            let prefix = format!("{path}/");
            edges.extend(
                self.files
                    .range::<str, _>((Bound::Included(prefix.as_str()), Bound::Unbounded))
                    .take_while(|entry| entry.starts_with(&prefix))
                    .copied()
                    .filter(|entry| !inputs::ancillary(entry) && allows(entry)),
            );
        } else {
            return false;
        }
        true
    }

    fn add_file(&self, path: &str, edges: &mut BTreeSet<&'a str>) -> bool {
        let Some(&path) = self.files.get(path) else {
            return false;
        };
        if !inputs::ancillary(path) {
            edges.insert(path);
        }
        true
    }

    fn extend_reference(
        &mut self,
        edges: &mut BTreeSet<&'a str>,
        path: &str,
        line: usize,
        raw: &str,
        base: &str,
        required: bool,
    ) {
        if self
            .resolve(base, raw)
            .is_some_and(|target| self.add_file(&target, edges))
            || !required
        {
            return;
        }
        self.missing(path, line, raw);
    }

    fn missing(&mut self, path: &str, line: usize, target: &str) {
        self.gaps.push(inputs::file_gap(
            path,
            line,
            format!("Unresolved file reference: {target}"),
        ));
    }

    pub(super) fn findings(&self, options: &Options) -> Outcome<Vec<Finding>> {
        let mut findings = Vec::new();
        for &path in self.files.difference(&self.reachable) {
            let Some(kind) = self.classification(path) else {
                continue;
            };
            if !options.includes(kind) || self.excluded(path) {
                continue;
            }
            findings.push(self.file_finding(path, kind)?);
        }
        Ok(findings)
    }

    pub(super) fn release_advisories(&self, targets: &[String]) -> Outcome<Vec<Advisory>> {
        let mut findings = Vec::new();
        for (&path, retained_by) in &self.release_assets {
            if self.excluded(path)
                || self.classification(path) != Some(Kind::Asset)
                || (!targets.is_empty() && !targets.iter().any(|target| inside(path, target)))
            {
                continue;
            }
            findings.push(Advisory {
                file: self.file_finding(path, Kind::Asset)?,
                reachable_from_runtime: self.runtime_release_assets.contains(path),
                retained_by: retained_by.clone(),
            });
        }
        findings.sort_by(|left, right| {
            right
                .file
                .bytes
                .cmp(&left.file.bytes)
                .then_with(|| left.file.path.cmp(&right.file.path))
        });
        Ok(findings)
    }

    fn excluded(&self, path: &str) -> bool {
        self.project
            .settings
            .excluded
            .iter()
            .any(|entry| inside(path, &entry.path))
    }

    fn file_finding(&self, path: &str, kind: Kind) -> Outcome<Finding> {
        let companions = ["import", "uid"]
            .into_iter()
            .map(|ext| format!("{path}.{ext}"))
            .filter(|sidecar| self.files.contains(sidecar.as_str()))
            .collect();
        Ok(Finding {
            kind,
            path: path.into(),
            item: None,
            line: 1,
            column: 1,
            bytes: (kind == Kind::Asset)
                .then(|| {
                    self.project
                        .root
                        .join(path)
                        .metadata()
                        .map(|file| file.len())
                })
                .transpose()?,
            companions,
        })
    }

    fn classification(&self, path: &str) -> Option<Kind> {
        let extension = inputs::extension(path).to_ascii_lowercase();
        match extension.as_str() {
            "gd" | "gdshader" | "gdshaderinc" | "tscn" | "tres" | "scn" | "res" | "escn" => {
                Some(Kind::File)
            }
            "png" | "webp" | "svg" | "jpg" | "jpeg" | "avif" | "bmp" | "tga" | "dds" | "exr"
            | "hdr" | "ktx" | "ktx2" | "ico" | "icns" | "gif" | "tiff" | "ogg" | "mp3" | "wav"
            | "ttf" | "otf" | "woff" | "woff2" | "glb" | "gltf" | "fbx" | "blend" | "dae"
            | "obj" | "po" | "ogv" => Some(Kind::Asset),
            _ if !inputs::ancillary(path)
                && self.files.contains(format!("{path}.import").as_str()) =>
            {
                Some(Kind::Asset)
            }
            _ => None,
        }
    }
}

fn record_reference(
    index: &mut release::Index<'_>,
    asset: &str,
    path: &str,
    kind: RetentionKind,
    reason: Option<&str>,
) {
    if let Some(references) = index.get_mut(asset) {
        references.insert(Retention {
            kind,
            path: path.into(),
            reason: reason.map(str::to_owned),
        });
    }
}

fn is_resource(raw: &str) -> bool {
    raw.starts_with("res://") || raw.starts_with("uid://")
}

fn is_opaque(extension: &str) -> bool {
    matches!(
        extension,
        "scn"
            | "res"
            | "cs"
            | "csproj"
            | "gdextension"
            | "gdc"
            | "pck"
            | "fbx"
            | "blend"
            | "dae"
            | "glb"
            | "obj"
    )
}
