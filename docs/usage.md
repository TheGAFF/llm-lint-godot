# Usage guide

Godot llm-lint is opinionated and pre-1.0. Upgrades can surface new findings in
existing projects. Pin the version in CI and review the release notes before
upgrading.

## Commands and configuration

- `check` runs architecture, native lint, formatting checks, and configured
  duplicate detection. `lint` omits the formatting check.
- `check --json` emits one report with `architecture`, `native` (lint and
  formatting findings), and `duplicates` (null when disabled).
- `architecture --json` and `duplicates --json` report findings for automation.
  Add paths relative to the project root to focus a check. Architecture expands
  targets to their owners and includes dependent living documents. Duplicate
  detection compares the target against the complete corpus.
- `rules --json` lists rule IDs and supported settings. The
  [rule reference](rules.md) includes executable examples.
- Add `--timings` to `check`, `lint`, `architecture`, or `duplicates` for a
  phase summary in milliseconds on stderr. The native phase includes style
  and, for `check`, formatting.
- `sync` updates generated `gdstyle.toml` after changes to
  `llm-lint.godot.toml`. `sync --check` checks for configuration drift.

Checks exit with 0 on success, 1 for error-level findings, and 2 for
configuration, input, or tool failures. Warnings are advisory. Set rule levels
to `off`, `warn`, or `error` for configurable rules. The last matching path
override wins for settings it declares. Run `sync` after configuration changes.

```toml
[rules]
"architecture/role-filename" = "error"

[[overrides]]
includes = ["src/features/ui/legacy/**"]
[overrides.rules]
"architecture/role-placement" = "warn"
```

### Adopt the linter in an existing project

1. Run `godot-llm-lint init --source-root <directory>` beside `project.godot`. Keep authored code under `<directory>/features/<feature>`, `<directory>/shared`, or `<directory>/app`. For multiple roots, edit `source_roots` after initialization.
2. Run `godot-llm-lint check --json` to identify existing findings. Use scoped overrides for areas you are migrating and the documented exclusions for vendored tooling and scratch.
3. Run `godot-llm-lint sync`, commit both configuration files, and run `godot-llm-lint check` again.

### Native tool setup and trust

To upgrade a project from gdstyle 0.3.1, run the upgraded linter's
`godot-llm-lint init` from the project root, then commit both configuration
files. This provisions 0.8.0, migrates either legacy `[gdstyle]` format to
the automatic pin, and regenerates `gdstyle.toml` with your project rules,
overrides, and thresholds. Run `godot-llm-lint check --json` to review new
findings, including extra spacing and other formatter changes.

`init` caches the official `gdstyle` at `gdstyle/0.8.0/<target>/` inside the linter's cache. The cache defaults to `%LOCALAPPDATA%/godot-llm-lint` on Windows, `~/Library/Caches/godot-llm-lint` on macOS, and `$XDG_CACHE_HOME/godot-llm-lint` or `~/.cache/godot-llm-lint` on Linux. A trusted caller can set `GODOT_LLM_LINT_CACHE_DIR` to an absolute path outside the checkout.

For offline setup, use `init --gdstyle-archive <file>` with the official archive, or `init --gdstyle-executable <external-file>` with the official executable. Both are checked against embedded SHA-256 digests. For a custom 0.8.0 build, supply `--gdstyle-executable <external-file> --gdstyle-sha256 <digest>` on each `init`, `check`, or `lint` invocation that uses it. Keep the executable and its digest in a trusted location outside the checkout. The caller supplies the authority to run native executables.

For trusted local projects, provision with `init`. For fork CI, pin the linter release and provision the native cache in an isolated runner using trusted release inputs. Run `sync --check` and `check --json` against the checkout with a read-only token and without secrets. Treat diagnostics and opt-in history as project data when sharing logs or artifacts. See the [security policy](../SECURITY.md).

### Excluding project paths

An `[[excluded]]` entry omits lint findings for the specified path and keeps its
classes and resource UIDs available to authored code. Use this default for
vendored addons. For a scratch copy or generated gallery, set `skip_discovery`
to prune the exact project-relative path and its contents before indexing.
References into that path cannot resolve through the project index.
Set `optional = true` when the path can be absent on a fresh checkout.

```toml
[[excluded]]
path = ".scratch"
reason = "Local project snapshots contain copies of authored resources."
optional = true
skip_discovery = true
```

### Keeping an empty authored directory

The [empty-directory rule](rules.md#architectureempty-directory) checks the
configured source, plugin, and tooling roots. A child directory or a file
other than `.gitkeep` or `.keep` counts as content. Remove empty directories
when they serve no purpose. For a directory with a planned use, record an
exact exception and track a `.gitkeep` if Git checkouts need the directory.

```toml
[[exceptions]]
path = "src/features/importer/testing/fixtures"
rule = "architecture/empty-directory"
item = "<directory>"
reason = "The importer integration test needs this path before it writes fixtures."
```

Remove the exception when the directory gains content or is removed.

### Scene and resource bundles

Projects can opt individual owners into subject folders. Place a scene,
its actor, helpers, art and tests under the subject's folder while preserving
the filename suffixes and the existing owner boundaries. Scripts continue to
use the role inferred from their suffix when checking class names, inheritance,
public static function prefixes, and collection size. Asset suffixes resolve
ambiguous media formats, and explicit asset classifications take precedence.
Keep Markdown under the owner's documentation role folders.

```toml
[bundles]
owners = ["src/features/ui"]
max_files = 30

[[bundles.companions]]
anchor = "src/features/ui/enemies/honey_tank/honey_tank_scene.tscn"
paths = [
  "src/features/ui/enemies/honey_tank/honey_tank_actor_node.gd",
  "src/features/ui/enemies/honey_tank/honey_tank_icon.svg",
]
```

The placement rule rejects managed files at the owner root and in owner-wide
role buckets such as `nodes/`. Registered entrypoints and native plugin
configuration may keep their conventional locations. Put tests under a
subject's `testing/` directory. The directory cap counts directly contained
Godot scripts, scenes, resources, media and fixtures, including localization
and shaders. UID sidecars, import sidecars, caches and maintained Markdown
are outside this count.

The companion check uses the complete reference graph. A scene or resource
may claim a script it attaches or an asset it references. A GDScript may
claim a functional script it references by path or class when its own
attachment or declaration establishes bundle membership. Inference requires
one resolved production consumer and the same owner. Independent scene and
resource files may be referenced from other bundles. Declare indirect relationships
with `[[bundles.companions]]`, for example for art loaded through a runtime
registry. A declared companion belongs beside its anchor or within its
directory subtree.

Each bundle rule is an error. Exact `[[bundles.exclusions]]` entries accept
a project-relative directory `path`, one bundle `rule`, and a concrete
technical `reason`. Size exclusions apply to the directory itself.
Placement and companion exclusions apply to findings for files within that
directory. A placement or companion diagnostic in GDScript, a text scene
or a text resource can use the existing line suppression syntax immediately
above the diagnostic line. Exclusions and comment directives that no longer
match a finding fail the check. Other checks still inspect excluded files.

```toml
[[bundles.exclusions]]
path = "src/features/ui/localization/catalog"
rule = "architecture/bundle-size"
reason = "Godot's locale catalog discovery requires the full locale set in one directory."
```

## Inspect references to a file

```sh
godot-llm-lint references src/features/ui/menu_scene.tscn
godot-llm-lint references res://src/features/ui/menu_scene.tscn --json
godot-llm-lint references uid://example --json --config llm-lint.godot.toml
```

Query one indexed file by project-relative path, absolute path within the
project, or UID. Relative paths resolve from the directory containing the lint
settings. The command reads the project without gdstyle, generated lint
configuration, or an import cache. It returns exit 0 for an indexed file with or
without references and exit 2 for an invalid target or a failed inventory scan.

Incoming and outgoing results show declaring locations and reference types.
The command follows static resource paths, GDScript loads and inheritance,
resolved project class and singleton symbols, scene and resource declarations,
project settings, plugin registrations, and living documents' Key Files rows.
An external resource with matching UID and saved path appears once. When the
saved path points elsewhere, it has its own entry. A file's UIDs appear with
the subject.

Possible results show the roots and extensions of directory references and
bounded dynamic loads. An incoming candidate must match a project root and its
extension filter. Unresolved reports show missing outbound targets and coverage
gaps identify computed calls without bounds or source syntax the parser could
not scan. The command uses static project data, so inspect coverage gaps when
using the output to plan a move or deletion.

Version 1 JSON includes the resolved subject (path, owner, UIDs), `incoming`,
`outgoing`, `possible`, `unresolved`, and `coverage_gaps`. Locations have
one-based lines and columns where available. Terminal output summarizes each
outgoing Key Files glob by pattern and match count. JSON contains the individual
matched files.

## Find unused content

```sh
godot-llm-lint unused --kind asset
godot-llm-lint unused src/features/ui/assets --kind asset --json
godot-llm-lint unused --fail-on-findings
```

`unused` is a read-only Godot 4 project audit and runs independently of gdstyle.
It follows resources and scripts from project settings, enabled editor plugins,
explicit export selections, configured entrypoints, and tests. By convention,
it starts from `test_*.gd`, `*_test_harness.gd`, and direct `GutTest` scripts in
`testing`, `test`, or `tests` folders. Registered tooling roots also contribute
shell, Python, and JavaScript/TypeScript scripts. Paths and `--kind`
filter findings after the project graph has been built. The kinds are `asset`,
`file`, `function`, and `variable`. Constants and locals use `variable`.

The asset report orders candidates by source size and lists their `.import` and
`.uid` companions. Regular exclusions hide findings and preserve dependency
resolution. Paths with `skip_discovery = true` are pruned before indexing. An
export preset that selects all resources leaves usage to the project graph.

Declare bounded runtime resource loads through `[[dynamic_loads]]`. Directory
enumeration alone leaves each file's usage to the project graph. Add active
manifests, HTML guides, or build inputs to `unused.reference_files` when their
usage comes from outside the discovered entry points. The audit follows exact
paths in quoted text and in JSON keys and values. Use `unused.keep` for externally
loaded paths or dynamic contracts the audit cannot infer.

When a loader selects only certain file types from an authored root, use the
optional `extensions = ["tres"]` list on its dynamic load declaration. Use
lowercase, dot-free extensions that match the loader's selection. Architecture
and the unused audit both apply the filter to the declared root.

```toml
[unused]
reference_files = ["tools/asset_registry.json"]
release_exclusion_files = ["tools/release_config.json"]

[[unused.keep]]
path = "src/features/content/assets/mod_icons"
reason = "External levels select icons by filename."

[[unused.keep]]
path = "src/app/nodes/startup_node.gd"
item = "_on_platform_resume"
reason = "Called by the native host."
```

Omit `item` to keep a file or directory and follow its dependencies. Use the
reported item name for symbol keeps, such as `InnerClass.method` or
`method.local_variable`.

Set `release_exclusion_files` to JSON manifests with an `asset_exclusions`
array of objects containing `path` and `reason`. The asset report lists
these entries by source size with their companions, exclusion reasons, and
the reachable project files that also refer to them. Runtime reachability
starts at runtime settings in `project.godot` and follows the same static
dependencies as the main audit. Inspect coverage gaps when reviewing that
result. Release advisories appear in `release_exclusions` in JSON and are
separate from the findings used by `--fail-on-findings`.

Review coverage gaps and the references behind each candidate before removing
it. The audit follows references from all declarations in reachable scripts,
so repeat it after pruning unreferenced code. Symbol findings describe missing
references. Review initializers and assignments for side effects when editing.
The audit recognizes Godot scene wiring and common reflective calls. Computed
paths, computed member names, and reachable opaque formats may leave gaps.

The JSON report includes a version, entrypoints, counts, diagnostics, and
coverage gaps. Advisory runs return `0` when the report completes. Strict runs
return `1` for findings, or `2` when a coverage gap affects the selected kind.
Invalid inputs also return `2`.

## Lint history

Record a full-project check with a model label. Use the printed run ID as an
explicit baseline for later work. The baseline can use a different model
label, such as `Before edits`.

```sh
godot-llm-lint check --record --model "GPT 6.2 Sol"
godot-llm-lint check --record --model "Claude Opus 5.5" --baseline-run 12
godot-llm-lint history report --model "Claude Opus 5.5" --since 2026-11-01
godot-llm-lint history report --run 13
godot-llm-lint history report --html lint-history.html
```

The history database lives at `.tools/godot-llm-lint/history.sqlite3`. Keep
it and any exported reports out of version control if they contain private
model labels or project paths. History retains rule IDs, effective severity,
and relative locations, including both ends of a duplicate finding. It
stores observations locally. HTML export writes a new file at the path you
provide and can be opened offline.

Pass rates and averages use completed runs. An incomplete run can have
findings from stages that finished before the tool failure. Comparisons use
the same lint configuration and tool version, and report observed changes
from the selected baseline. The `--since` date starts at midnight UTC.

## Ownership and file roles

Keep scripts, resources, scenes, assets, tests, and project documentation with
their feature, app, shared, or registered plugin or tool owner. Use the
[file-role catalog](../policy/file-roles.json) for folders and suffixes, and the
[functional-role catalog](../policy/functional-roles.json) for public static
GDScript functions. Example paths relative to an owner include
`resources/levels/first_level_resource.tres`, `scenes/actors/player_scene.tscn`,
`assets/icons/player_icon.svg`, and `testing/fixtures/saves_fixture.json`.
Use lowercase snake_case and include a letter in the filename's subject.

Checks inventory unreferenced assets and use scene/resource references and
import UIDs in the ownership graph. On a role failure, inspect `expected` for
a suggested destination and review any collision report before moving files.
Move `.import` and `.uid` companions with the source, update references, and
verify the project still imports. JSON, CSV, and text fixtures are classified
under `testing`. Ordinary tooling manifests and licenses are support files.

Assign an explicit role to an ambiguous asset when needed. The selected role's
folder and filename requirements still apply.

```toml
[[asset_classifications]]
includes = ["src/features/ui/assets/legacy_buttons/*.svg"]
role = "icon"
```

## Mapper and companion contracts

`architecture/mapper-return-type` checks that a `map_` function names its
declared output. Use `map_user_dto` for `UserDto` and `map_user_dtos` for
`Array[UserDto]`. Built-in value types and dictionaries are exempt.
`architecture/mapper-output-declaration` checks that a named output resolves to
an available class. It recognizes local and global declarations, preload aliases,
and engine classes. Configure exact external class names and irregular plurals
when the project needs them.

```toml
[rules."architecture/mapper-return-type".options]
pluralForms = { Person = "People" }

[rules."architecture/mapper-output-declaration".options]
externalTypes = ["ExtensionView"]

[rules."architecture/companion-files".options.pairs.mapper-notes]
match = "src/**/{name}.gd"
require = ["${name}.md"]
```

Companion pairs are opt-in. Each pair has a stable name, a project-relative
match, and required paths relative to the matched file's directory. Use
`{name}` to capture within a segment and `${name}` to substitute that capture. `*` and `?`
stay within one segment, and `**` crosses directories. Use exact casing and
slash-separated paths. Required paths must resolve to files inside the project.
Named pairs and irregular plurals merge by key through overrides. Set an
individual pair to `false` to disable it in a later layer.

## Maintained Markdown

Place each document under its owner in the role folder and use the corresponding
suffix. Examples include `docs/guides/save_guide.md` and
`docs/verification/save_verification.md`.

| Role folder | Suffix | Body sections in order |
| --- | --- | --- |
| `specifications/` | `_specification.md` | Requirements and Acceptance Criteria |
| `guides/` | `_guide.md` | Prerequisites, Steps, and Validation |
| `references/` | `_reference.md` | Entries |
| `standards/` | `_standard.md` | Rules and Validation |
| `decisions/` | `_decision.md` | Decision, Rationale, Alternatives, and Consequences |
| `verification/` | `_verification.md` | Results and Limitations |

Start with `+++` TOML front matter containing a quoted `reviewed` date
(`YYYY-MM-DD`, UTC, at or before today). Living specifications, guides,
references, and standards also require a `sources_hash`. Source freshness
checks apply to these living roles. Decisions and verification records document
historical snapshots.

Use one H1, then an H2 `Summary` with one paragraph of up to 80 words and
`Key Files` with a `Path | Purpose` table. Add the role's H2 body sections and
finish with `Related Documents`, an unordered list of Markdown links. Body
sections use unordered lists except numbered `Steps`, `Entries` tables, and
the verification `Results` table with `Date | Check | Result | Evidence` columns.
H3s can group body content. The default body limits are 150 lines and 1,500
words, or 100 lines and 1,000 words for decisions and verification records.

Use exact project-relative paths or supported globs in Key Files. Live
declarations must match existing files and may refer to other owners. When the
declared paths or contents change, review the prose and restamp the file.

```sh
godot-llm-lint docs review src/features/run/docs/guides/save_guide.md --reviewed YYYY-MM-DD
godot-llm-lint architecture --json
```

`docs review` validates explicit Markdown paths before updating their review
metadata. Commit the generated companion at
`.llm-lint/doc-reviews/<document-path>.json` with each living document's review.
Keep it with the document when moving or deleting the document. Checks compare
the reviewed file hashes with the current Key Files scope and report modified,
newly included, and no-longer-included paths. JSON exposes the explanation as
`document_sources`. After deleting a declared source, run a full check or
target its former parent directory.

For a legacy review, pass `--doc-baseline <ref>` to `architecture`, `check`, or
`lint`. Use a local Git revision containing the reviewed document and its
sources. The checker prefers a valid companion and verifies that the selected
revision reproduces the recorded fingerprint before attributing any changes.

The root README, `AGENTS.md`, and the exact legal Markdown basenames
`LICENSE.md`, `COPYING.md`, and `NOTICE.md` may keep their native placement.
Use a narrow documentation exclusion for generated or vendored Markdown.

```toml
[[documentation_exclusions]]
includes = ["vendor/**/*.md"]
reason = "Upstream documentation"
```

## Duplicate evidence and reviewed exceptions

Review warnings before extracting shared code. Exact matches default to a warning
at 50 tokens and five code lines, and an error at 100 tokens and ten code lines.
Renamed matching substitutes lexically bound parameters and locals while
preserving external names and literals. Structural candidates must also meet the
ordered semantic-token threshold and agree on their operator traces.

Terminal reports count active warnings once across production, test, and
capture. Any production endpoint places the pair in production. Test takes
precedence over capture for the remaining mixed pairs. Capture directories
and capture-script naming take precedence over test naming. Pass
`--duplicate-warnings production|test|capture|all` to `duplicates`, `check`,
or `lint` for individual warning pairs. JSON includes
`summary.warnings_by_category` plus categories on each pair and endpoint.

For an intentional warning, use the exact occurrence paths, function names,
rules, and 64-character lowercase fingerprints from `duplicates --json`.

```toml
[[duplicate_exceptions]]
id = "independent_capture_scenarios"
rules = ["duplicates/exact"]
reason = "Keep each capture scenario's independent expected values."
members = [
  { path = "src/app/testing/one_test_harness.gd", item = "jobs", fingerprint = "<first fingerprint>" },
  { path = "src/app/testing/two_test_harness.gd", item = "jobs", fingerprint = "<second fingerprint>" },
]
```

Every occurrence must match the same reviewed entry. New occurrences still
report a finding. Changed functions, unused rules or members, and overlapping
entries fail validation, including on targeted runs. Error-level findings must
be fixed at the source. JSON exposes approved findings in `suppressed`, while
`diagnostics` and active counts describe the remaining findings.
