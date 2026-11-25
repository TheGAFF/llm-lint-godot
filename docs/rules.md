# Rule reference

Generated from the rule catalog and executable examples. After changing either source, regenerate this file with `cargo test --locked --test rule_examples regenerate_rule_documentation -- --ignored`. Use `godot-llm-lint rules --json` to inspect rule IDs.

Custom and individually selected native rules have passing/failing fixtures. Rules inherited from pinned upstream groups retain upstream coverage and local group integration tests.

## architecture/asset-collision

Suggested asset destinations must not overwrite existing files.

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--asset-collision.json).

### Passing

`llm-lint.godot.toml`

```text
version=1
```

### Failing

`src/features/ui/assets/music/music_addition.ogg`

```text

```

## architecture/asset-kind

Assets must have an unambiguous supported role; classify ambiguous media explicitly.

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--asset-kind.json).

### Passing

`llm-lint.godot.toml`

```text
version=1
```

### Failing

`src/features/ui/assets/play.svg`

```text

```

## architecture/bundle-companion

Exclusive companions must live beside their resolved scene, resource or script anchor.

Default: **error**. Input integrity: always an error.

[Complete executable fixture](../examples/rules/architecture--bundle-companion.json).

### Passing

`src/shared/subject/sample_node.gd`

```gdscript
extends Node
```

### Failing

`src/shared/components/sample_node.gd`

```gdscript
extends Node
```

## architecture/bundle-configuration

Companion declarations and directory exclusions must reference existing, applicable bundle subjects.

Default: **error**. Input integrity: always an error.

[Complete executable fixture](../examples/rules/architecture--bundle-configuration.json).

### Passing

`llm-lint.godot.toml`

```text
version = 1
[bundles]
owners = ['src/shared']
```

### Failing

`llm-lint.godot.toml`

```text
version = 1
[bundles]
owners = ['src/shared']
[[bundles.companions]]
anchor = 'src/shared/subject/sample_scene.tscn'
paths = ['src/shared/subject/sample_node.gd']
```

## architecture/bundle-placement

Bundle-enabled owners put managed Godot files under a subject directory.

Default: **error**. Input integrity: always an error.

[Complete executable fixture](../examples/rules/architecture--bundle-placement.json).

### Passing

`src/shared/subject/sample_node.gd`

```gdscript
extends Node
```

### Failing

`src/shared/nodes/sample_node.gd`

```gdscript
extends Node
```

## architecture/bundle-size

Bundle-enabled directories must stay within the direct authored Godot file cap.

Default: **error**. Input integrity: always an error.

[Complete executable fixture](../examples/rules/architecture--bundle-size.json).

### Passing

`src/shared/subject/sample_node.gd`

```gdscript
extends Node
```

### Failing

`src/shared/subject/other_node.gd`

```gdscript
extends Node
```

## architecture/class-name

ReadRunQuery

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--class-name.json).

### Passing

`src/features/run/queries/read_run_query.gd`

```gdscript
class_name ReadRunQuery
extends RefCounted
static func get_run() -> int:
	return 1
```

### Failing

`src/features/run/queries/read_run_query.gd`

```gdscript
class_name Wrong
extends Node
static func validate_run() -> bool:
	return true
```

## architecture/class-unique

One globally unique class_name

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--class-unique.json).

### Passing

`addons/vendor/test.gd`

```gdscript
class_name VendorTest
extends Node
func badlyNamed() -> void:
	pass
```

### Failing

`addons/vendor/test.gd`

```gdscript
class_name VendorTest
extends Node
func badlyNamed() -> void:
	pass
```

## architecture/collection-size

At most 2 code lines

Default: **error**. Levels: off, warn, error.

Options: `max`.

[Complete executable fixture](../examples/rules/architecture--collection-size.json).

### Passing

`src/shared/helpers/value_helpers.gd`

```gdscript
# reason:
# gdstyle:ignore=quality/type-hint
extends RefCounted
```

### Failing

`src/features/run/run_queries.gd`

```gdscript
class_name RunQueries
extends RefCounted
static func get_run() -> int:
	return 1
```

## architecture/comment-suppression

Next-line suppressions name one configurable architecture rule and a nonempty reason, and must suppress a diagnostic.

Default: **error**. Input integrity: always an error.

[Complete executable fixture](../examples/rules/architecture--comment-suppression.json).

### Passing

`src/features/content/resources/base_resource.gd`

```gdscript
class_name BaseResource
extends Resource
@export var id: StringName = &"base"
```

### Failing

`src/features/content/resources/base_resource.gd`

```gdscript
class_name BaseResource
extends Resource
# llm-lint-ignore architecture/resource-id-type: Unused suppression.
@export var id: StringName = &"base"
```

## architecture/companion-files

Matched source files require all configured companion files with exact casing.

Default: **error**. Levels: off, warn, error.

Options: `pairs`.

[Complete executable fixture](../examples/rules/architecture--companion-files.json).

### Passing

`src/shared/mappers.gd`

```gdscript
class UserDto:
	extends RefCounted
static func map_user_dto() -> UserDto:
	return UserDto.new()
```

### Failing

`src/shared/mappers.gd`

```gdscript
class UserDto:
	extends RefCounted
static func map_user_dto() -> UserDto:
	return UserDto.new()
```

## architecture/dependency-direction

shared -> shared; features -> shared/features; runtime cannot load testing/editor/tooling

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--dependency-direction.json).

### Passing

`src/shared/helpers/value_helpers.gd`

```gdscript
# reason:
# gdstyle:ignore=quality/type-hint
extends RefCounted
```

### Failing

`src/features/run/models/run_model.gd`

```gdscript
extends RefCounted
const TEST = preload("res://src/features/run/testing/fixture.gd")
```

## architecture/document-blueprint

Each document follows its role-specific ordered heading blueprint.

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--document-blueprint.json).

### Passing

`src/features/run/docs/decisions/checkpoint_decision.md`

```text
+++
reviewed = "2026-01-01"
+++
# Checkpoint

## Summary

Checkpoint behavior and its review evidence.

## Key Files

| Path | Purpose |
| --- | --- |
| `project.godot` | Project settings. |

## Decision

- Preserve saves.

## Rationale

- Resume interrupted play.

## Alternatives

- Restarting was rejected.

## Consequences

- Save compatibility is required.

## Related Documents

- [Project](README.md).
```

### Failing

`src/features/run/docs/decisions/checkpoint_decision.md`

```text
+++
reviewed = "2026-01-01"
+++
# Checkpoint

## Summary

Checkpoint behavior and its review evidence.

## Key Files

| Path | Purpose |
| --- | --- |
| `project.godot` | Project settings. |

## Decision

- Preserve saves.

## Explanation

- Resume interrupted play.

## Alternatives

- Restarting was rejected.

## Consequences

- Save compatibility is required.

## Related Documents

- [Project](README.md).
```

## architecture/document-collision

Suggested document destinations must not overwrite existing files or share a destination.

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--document-collision.json).

### Passing

`llm-lint.godot.toml`

```text
version=1
```

### Failing

`src/features/run/docs/guides/save.md`

```text
+++
reviewed = "2026-01-01"
sources_hash = "sha256:9b02d4924bd0ee0415d68dfe6348440aa508fdbdebdc270960c10a3b15196237"
+++
# Checkpoint

## Summary

Checkpoint behavior and its review evidence.

## Key Files

| Path | Purpose |
| --- | --- |
| `project.godot` | Project settings. |

## Prerequisites

- A configured project.

## Steps

1. Resume the run.

## Validation

- Confirm the saved run resumes.

## Related Documents

- [Project](README.md).
```

## architecture/document-metadata

Role documents require valid, nonfuture reviewed dates and strict TOML review metadata.

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--document-metadata.json).

### Passing

`src/features/run/docs/decisions/checkpoint_decision.md`

```text
+++
reviewed = "2026-01-01"
+++
# Checkpoint

## Summary

Checkpoint behavior and its review evidence.

## Key Files

| Path | Purpose |
| --- | --- |
| `project.godot` | Project settings. |

## Decision

- Preserve saves.

## Rationale

- Resume interrupted play.

## Alternatives

- Restarting was rejected.

## Consequences

- Save compatibility is required.

## Related Documents

- [Project](README.md).
```

### Failing

`src/features/run/docs/decisions/checkpoint_decision.md`

```text
+++
reviewed = "9999-01-01"
+++
# Checkpoint

## Summary

Checkpoint behavior and its review evidence.

## Key Files

| Path | Purpose |
| --- | --- |
| `project.godot` | Project settings. |

## Decision

- Preserve saves.

## Rationale

- Resume interrupted play.

## Alternatives

- Restarting was rejected.

## Consequences

- Save compatibility is required.

## Related Documents

- [Project](README.md).
```

## architecture/document-role

Markdown documents require a specification, guide, reference, standard, decision or verification role.

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--document-role.json).

### Passing

`llm-lint.godot.toml`

```text
version=1
```

### Failing

`src/features/run/docs/dump.md`

```text
# Run
```

## architecture/document-size

Documents stay within role-specific line and word budgets; remove repetition or split focused topics.

Default: **error**. Levels: off, warn, error.

Options: `max_lines`, `max_words`.

[Complete executable fixture](../examples/rules/architecture--document-size.json).

### Passing

`src/features/run/docs/decisions/checkpoint_decision.md`

```text
+++
reviewed = "2026-01-01"
+++
# Checkpoint

## Summary

Checkpoint behavior and its review evidence.

## Key Files

| Path | Purpose |
| --- | --- |
| `project.godot` | Project settings. |

## Decision

- Preserve saves.

## Rationale

- Resume interrupted play.

## Alternatives

- Restarting was rejected.

## Consequences

- Save compatibility is required.

## Related Documents

- [Project](README.md).
```

### Failing

`src/features/run/docs/decisions/checkpoint_decision.md`

```text
+++
reviewed = "2026-01-01"
+++
# Checkpoint

## Summary

Checkpoint behavior and its review evidence.

## Key Files

| Path | Purpose |
| --- | --- |
| `project.godot` | Project settings. |

## Decision

- Preserve saves.

## Rationale

- word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word

## Alternatives

- Restarting was rejected.

## Consequences

- Save compatibility is required.

## Related Documents

- [Project](README.md).
```

## architecture/document-sources

Key Files declares meaningful, safe, case-sensitive source files or globs.

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--document-sources.json).

### Passing

`src/features/run/docs/guides/checkpoint_guide.md`

```text
+++
reviewed = "2026-01-01"
sources_hash = "sha256:9b02d4924bd0ee0415d68dfe6348440aa508fdbdebdc270960c10a3b15196237"
+++
# Checkpoint

## Summary

Checkpoint behavior and its review evidence.

## Key Files

| Path | Purpose |
| --- | --- |
| `project.godot` | Project settings. |

## Prerequisites

- A configured project.

## Steps

1. Resume the run.

## Validation

- Confirm the saved run resumes.

## Related Documents

- [Project](README.md).
```

### Failing

`src/features/run/docs/guides/checkpoint_guide.md`

```text
+++
reviewed = "2026-01-01"
sources_hash = "sha256:9b02d4924bd0ee0415d68dfe6348440aa508fdbdebdc270960c10a3b15196237"
+++
# Checkpoint

## Summary

Checkpoint behavior and its review evidence.

## Key Files

| Path | Purpose |
| --- | --- |
| `../outside.txt` | Project settings. |

## Prerequisites

- A configured project.

## Steps

1. Resume the run.

## Validation

- Confirm the saved run resumes.

## Related Documents

- [Project](README.md).
```

## architecture/document-stale

Living documents must be reviewed when their declared source contents or file sets change.

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--document-stale.json).

### Passing

`src/features/run/docs/guides/checkpoint_guide.md`

```text
+++
reviewed = "2026-01-01"
sources_hash = "sha256:9b02d4924bd0ee0415d68dfe6348440aa508fdbdebdc270960c10a3b15196237"
+++
# Checkpoint

## Summary

Checkpoint behavior and its review evidence.

## Key Files

| Path | Purpose |
| --- | --- |
| `project.godot` | Project settings. |

## Prerequisites

- A configured project.

## Steps

1. Resume the run.

## Validation

- Confirm the saved run resumes.

## Related Documents

- [Project](README.md).
```

### Failing

`src/features/run/docs/guides/checkpoint_guide.md`

```text
+++
reviewed = "2026-01-01"
sources_hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
+++
# Checkpoint

## Summary

Checkpoint behavior and its review evidence.

## Key Files

| Path | Purpose |
| --- | --- |
| `project.godot` | Project settings. |

## Prerequisites

- A configured project.

## Steps

1. Resume the run.

## Validation

- Confirm the saved run resumes.

## Related Documents

- [Project](README.md).
```

## architecture/document-structure

Role sections use concise prose, ordered or unordered lists, and defined tables.

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--document-structure.json).

### Passing

`src/features/run/docs/guides/checkpoint_guide.md`

```text
+++
reviewed = "2026-01-01"
sources_hash = "sha256:9b02d4924bd0ee0415d68dfe6348440aa508fdbdebdc270960c10a3b15196237"
+++
# Checkpoint

## Summary

Checkpoint behavior and its review evidence.

## Key Files

| Path | Purpose |
| --- | --- |
| `project.godot` | Project settings. |

## Prerequisites

- A configured project.

## Steps

1. Resume the run.

## Validation

- Confirm the saved run resumes.

## Related Documents

- [Project](README.md).
```

### Failing

`src/features/run/docs/guides/checkpoint_guide.md`

```text
+++
reviewed = "2026-01-01"
sources_hash = "sha256:9b02d4924bd0ee0415d68dfe6348440aa508fdbdebdc270960c10a3b15196237"
+++
# Checkpoint

## Summary

Checkpoint behavior and its review evidence.

## Key Files

| Path | Purpose |
| --- | --- |
| `project.godot` | Project settings. |

## Prerequisites

- A configured project.

## Steps

- Resume the run.

## Validation

- Confirm the saved run resumes.

## Related Documents

- [Project](README.md).
```

## architecture/dynamic-load

Literal targets or a reasoned function-to-resource-root declaration

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--dynamic-load.json).

### Passing

`src/shared/queries/load_query.gd`

```gdscript
extends RefCounted
static func load_value(path: String) -> Resource:
	return load(path)
```

### Failing

`src/shared/queries/load_query.gd`

```gdscript
extends RefCounted
static func load_value(path: String) -> Resource:
	return load(path)
```

## architecture/dynamic-load-bound

Load target within declared runtime roots

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--dynamic-load-bound.json).

### Passing

`src/features/run/queries/load_run_query.gd`

```gdscript
extends RefCounted
```

### Failing

`src/features/run/queries/load_run_query.gd`

```gdscript
extends RefCounted
static func load_run(path: String) -> Resource:
	return load('user://other/run_resource.tres')
```

## architecture/dynamic-load-stale

Remove stale declaration

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--dynamic-load-stale.json).

### Passing

`src/features/run/queries/load_run_query.gd`

```gdscript
extends RefCounted
static func load_run(path: String) -> Resource:
	return load(path)
```

### Failing

`src/features/run/queries/load_run_query.gd`

```gdscript
extends RefCounted
```

## architecture/empty-directory

Authored directories must contain a child directory or a file other than .gitkeep or .keep. Use an exact exception with item <directory> for a planned use.

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--empty-directory.json).

### Passing

`llm-lint.godot.toml`

```text
version = 1
[[exceptions]]
path = "src/shared/fixtures"
rule = "architecture/empty-directory"
item = "<directory>"
reason = "The fixture writer expects the directory to exist before running."
```

### Failing

`llm-lint.godot.toml`

```text
version = 1
```

## architecture/exception-stale

Remove unused exact exception

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--exception-stale.json).

### Passing

`src/features/run/models/run_model.gd`

```gdscript
class_name Wrong
extends RefCounted
```

### Failing

`src/features/run/models/run_model.gd`

```gdscript
extends RefCounted
```

## architecture/feature-layout

features/<owner>, shared, app or a registered plugin/tool owner

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--feature-layout.json).

### Passing

`src/shared/helpers/value_helpers.gd`

```gdscript
# reason:
# gdstyle:ignore=quality/type-hint
extends RefCounted
```

### Failing

`other.gd`

```gdscript
extends Node
```

## architecture/file-name

snake_case paths

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--file-name.json).

### Passing

`src/shared/helpers/value_helpers.gd`

```gdscript
# reason:
# gdstyle:ignore=quality/type-hint
extends RefCounted
```

### Failing

`src/features/run/testing/Run.gd`

```gdscript
extends Node
```

## architecture/function-role

validators

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--function-role.json).

### Passing

`src/features/run/queries/read_run_query.gd`

```gdscript
class_name ReadRunQuery
extends RefCounted
static func get_run() -> int:
	return 1
```

### Failing

`src/features/run/queries/read_run_query.gd`

```gdscript
class_name Wrong
extends Node
static func validate_run() -> bool:
	return true
```

## architecture/local-variable-snake-case

Function-local variables and loop and match bindings use snake_case without a leading underscore.

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--local-variable-snake-case.json).

### Passing

`src/features/gameplay/nodes/spawner_node.gd`

```gdscript
class_name SpawnerNode
extends Node

func _ready() -> void:
	var spawn_cooldown: float = 2.5
	print(spawn_cooldown)
```

### Failing

`src/features/gameplay/nodes/spawner_node.gd`

```gdscript
class_name SpawnerNode
extends Node

func _ready() -> void:
	var _spawn_cooldown: float = 2.5
	print(_spawn_cooldown)
```

## architecture/mapper-output-declaration

Named mapper outputs resolve to local, shared, or known external declarations.

Default: **error**. Levels: off, warn, error.

Options: `externalTypes`.

[Complete executable fixture](../examples/rules/architecture--mapper-output-declaration.json).

### Passing

`src/shared/mappers.gd`

```gdscript
class UserDto:
	extends RefCounted
static func map_user_dto() -> UserDto:
	return UserDto.new()
```

### Failing

`src/shared/mappers.gd`

```gdscript
static func map_user_dto() -> UserDto:
	return UserDto.new()
```

## architecture/mapper-return-type

Mapper names identify their named output and plural collection payload.

Default: **error**. Levels: off, warn, error.

Options: `pluralForms`.

[Complete executable fixture](../examples/rules/architecture--mapper-return-type.json).

### Passing

`src/shared/mappers.gd`

```gdscript
class UserDto:
	extends RefCounted
static func map_user_dto() -> UserDto:
	return UserDto.new()
```

### Failing

`src/shared/mappers.gd`

```gdscript
class UserDto:
	extends RefCounted
static func map_wrong() -> UserDto:
	return UserDto.new()
```

## architecture/member-underscore-required

Methods and member variables used only in their own script file start with an underscore.

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--member-underscore-required.json).

### Passing

`src/features/gameplay/nodes/spawner_node.gd`

```gdscript
class_name SpawnerNode
extends Node

var _spawn_cooldown: float = 2.5

func _recalculate_path() -> void:
	_spawn_cooldown = 1.0

func _ready() -> void:
	_recalculate_path()
```

### Failing

`src/features/gameplay/nodes/spawner_node.gd`

```gdscript
class_name SpawnerNode
extends Node

var spawn_cooldown: float = 2.5

func recalculate_path() -> void:
	spawn_cooldown = 1.0

func _ready() -> void:
	recalculate_path()
```

## architecture/native-suppression

One exact gdstyle rule with a same-line or adjacent '# reason: <explanation>'; no file-wide suppression

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--native-suppression.json).

### Passing

`src/shared/helpers/value_helpers.gd`

```gdscript
# reason: Separate mutable instances
# gdstyle:ignore=quality/allocation-in-loop
extends RefCounted
```

### Failing

`src/shared/helpers/value_helpers.gd`

```gdscript
# reason:
# gdstyle:ignore=quality/type-hint
extends RefCounted
```

## architecture/private-member-access

Calls, callable references, reads, and writes of underscore-prefixed script members stay within their declaring file, with an exception for inherited hooks.

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--private-member-access.json).

### Passing

`src/features/ui/nodes/preview_node.gd`

```gdscript
class_name PreviewNode
extends Node

func refresh(spawner: SpawnerNode) -> void:
	spawner.recalculate_path()
```

### Failing

`src/features/ui/nodes/preview_node.gd`

```gdscript
class_name PreviewNode
extends Node

func refresh(spawner: SpawnerNode) -> void:
	spawner._recalculate_path()
```

## architecture/reference

Existing project-relative reference with exact path casing

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--reference.json).

### Passing

`src/app/scenes/main_scene.tscn`

```text
[gd_scene format=3]
[ext_resource type="Resource" path="uid://world1" id="1"]
```

### Failing

`src/app/scenes/main_scene.tscn`

```text
[gd_scene format=3]
[ext_resource path="uid://missing"]
```

## architecture/resource-id-name

Standalone scripted resource IDs match the filename without the extension and final _resource suffix, and are globally unique.

Default: **off**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--resource-id-name.json).

### Passing

`src/features/content/resources/base_resource.tres`

```text
[gd_resource type="Resource" format=3]
[ext_resource type="Script" path="res://src/features/content/resources/base_resource.gd" id="s"]
[resource]
script = ExtResource("s")
id = &"base"
```

### Failing

`src/features/content/resources/base_resource.tres`

```text
[gd_resource type="Resource" format=3]
[ext_resource type="Script" path="res://src/features/content/resources/base_resource.gd" id="s"]
[resource]
script = ExtResource("s")
id = &"wrong"
```

## architecture/resource-id-type

Primary exported resource IDs use StringName; numeric IDs are excluded.

Default: **off**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--resource-id-type.json).

### Passing

`src/features/content/resources/base_resource.gd`

```gdscript
class_name BaseResource
extends Resource
@export var id: StringName = &"base"
```

### Failing

`src/features/content/resources/base_resource.gd`

```gdscript
class_name BaseResource
extends Resource
@export var id: String = "base"
```

## architecture/role-base

RefCounted inheritance

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--role-base.json).

### Passing

`src/shared/helpers/value_helpers.gd`

```gdscript
# reason:
# gdstyle:ignore=quality/type-hint
extends RefCounted
```

### Failing

`src/features/run/models/run_model.gd`

```gdscript
extends Node
```

## architecture/role-filename

queries/<subject>_query.gd

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--role-filename.json).

### Passing

`src/shared/helpers/value_helpers.gd`

```gdscript
# reason:
# gdstyle:ignore=quality/type-hint
extends RefCounted
```

### Failing

`src/features/run/queries/run.gd`

```gdscript
extends RefCounted
static func get_run() -> int:
	return 1
```

## architecture/role-layout-conflict

Collection file XOR role directory

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--role-layout-conflict.json).

### Passing

`src/shared/helpers/value_helpers.gd`

```gdscript
# reason:
# gdstyle:ignore=quality/type-hint
extends RefCounted
```

### Failing

`src/features/run/run_queries.gd`

```gdscript
class_name RunQueries
extends RefCounted
static func get_run() -> int:
	return 1
```

## architecture/role-placement

Role folder with matching suffix, or owner-level functional collection

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--role-placement.json).

### Passing

`src/shared/helpers/value_helpers.gd`

```gdscript
# reason:
# gdstyle:ignore=quality/type-hint
extends RefCounted
```

### Failing

`other.gd`

```gdscript
extends Node
```

## architecture/stringname-literal

Use & literals in statically established StringName contexts; dynamic conversions are allowed.

Default: **off**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--stringname-literal.json).

### Passing

`src/features/content/resources/base_resource.gd`

```gdscript
class_name BaseResource
extends Resource
@export var id: StringName = &"base"
```

### Failing

`src/features/content/resources/base_resource.gd`

```gdscript
class_name BaseResource
extends Resource
@export var id: StringName = "base"
```

## architecture/syntax

Syntax

Default: **error**. Input integrity: always an error.

[Complete executable fixture](../examples/rules/architecture--syntax.json).

### Passing

`src/shared/queries/value_query.gd`

```gdscript
extends RefCounted
static func get_value() -> int:
	return 1
```

### Failing

`src/shared/queries/value_query.gd`

```gdscript
extends RefCounted
static func get_value( -> int:
	return 1
```

## architecture/testing-name

test_*.gd, *_test_factory.gd or *_test_harness.gd

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--testing-name.json).

### Passing

`src/shared/helpers/value_helpers.gd`

```gdscript
# reason:
# gdstyle:ignore=quality/type-hint
extends RefCounted
```

### Failing

`src/features/run/testing/Run.gd`

```gdscript
extends Node
```

## architecture/utility-state

Static functions with private implementation helpers

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/architecture--utility-state.json).

### Passing

`src/shared/helpers/value_helpers.gd`

```gdscript
# reason:
# gdstyle:ignore=quality/type-hint
extends RefCounted
```

### Failing

`src/features/run/queries/other_query.gd`

```gdscript
extends RefCounted
var count: int = 0
func read() -> int:
	return count
```

## duplicates/exact

Exact

Default: **threshold**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/duplicates--exact.json).

### Passing

`src/features/ui/nodes/one_node.gd`

```gdscript
extends Node
func calculate_one(input: int) -> int:
	var total: int = input + 8
	if total > 10:
		total += 7
	return total * 3
```

### Failing

`src/features/run/calculators/one_calculator.gd`

```gdscript
extends RefCounted
static func compute_one(input: int) -> int:
	var total: int = input
	total = (total + 0) * 2 - input
	total = (total + 1) * 2 - input
	total = (total + 2) * 2 - input
	total = (total + 3) * 2 - input
	total = (total + 4) * 2 - input
	total = (total + 5) * 2 - input
	total = (total + 6) * 2 - input
	total = (total + 7) * 2 - input
	total = (total + 8) * 2 - input
	total = (total + 9) * 2 - input
	total = (total + 10) * 2 - input
	total = (total + 11) * 2 - input
	total = (total + 12) * 2 - input
	total = (total + 13) * 2 - input
	total = (total + 14) * 2 - input
	total = (total + 15) * 2 - input
	total = (total + 16) * 2 - input
	total = (total + 17) * 2 - input
	return total
static func compute_two(input: int) -> int:
	var total: int = input
	total = (total + 0) * 2 - input
	total = (total + 1) * 2 - input
	total = (total + 2) * 2 - input
	total = (total + 3) * 2 - input
	total = (total + 4) * 2 - input
	total = (total + 5) * 2 - input
	total = (total + 6) * 2 - input
	total = (total + 7) * 2 - input
	total = (total + 8) * 2 - input
	total = (total + 9) * 2 - input
	total = (total + 10) * 2 - input
	total = (total + 11) * 2 - input
	total = (total + 12) * 2 - input
	total = (total + 13) * 2 - input
	total = (total + 14) * 2 - input
	total = (total + 15) * 2 - input
	total = (total + 16) * 2 - input
	total = (total + 17) * 2 - input
	return total
```

## duplicates/renamed

Renamed

Default: **threshold**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/duplicates--renamed.json).

### Passing

`src/features/one/calculators/value_calculator.gd`

```gdscript
extends RefCounted
static func compute_value(input: int) -> int:
	var total: int = input
	total = (total + 0) * 2 - input
	total = (total + 1) * 2 - input
	total = (total + 2) * 2 - input
	total = (total + 3) * 2 - input
	total = (total + 4) * 2 - input
	total = (total + 5) * 2 - input
	total = (total + 6) * 2 - input
	total = (total + 7) * 2 - input
	total = (total + 8) * 2 - input
	total = (total + 9) * 2 - input
	total = (total + 10) * 2 - input
	total = (total + 11) * 2 - input
	total = (total + 12) * 2 - input
	total = (total + 13) * 2 - input
	total = (total + 14) * 2 - input
	total = (total + 15) * 2 - input
	total = (total + 16) * 2 - input
	total = (total + 17) * 2 - input
	return total
```

### Failing

`src/features/one/calculators/value_calculator.gd`

```gdscript
extends RefCounted
static func compute_value(input: int) -> int:
	var total: int = input
	total = (total + 0) * 2 - input
	total = (total + 1) * 2 - input
	total = (total + 2) * 2 - input
	total = (total + 3) * 2 - input
	total = (total + 4) * 2 - input
	total = (total + 5) * 2 - input
	total = (total + 6) * 2 - input
	total = (total + 7) * 2 - input
	total = (total + 8) * 2 - input
	total = (total + 9) * 2 - input
	total = (total + 10) * 2 - input
	total = (total + 11) * 2 - input
	total = (total + 12) * 2 - input
	total = (total + 13) * 2 - input
	total = (total + 14) * 2 - input
	total = (total + 15) * 2 - input
	total = (total + 16) * 2 - input
	total = (total + 17) * 2 - input
	return total
```

## duplicates/similar

Similar

Default: **threshold**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/duplicates--similar.json).

### Passing

`src/features/content/resources/one_resource.tres`

```text
[gd_resource format=3]
[resource]
values=[1,2,3,4,5]
```

### Failing

`src/features/ui/nodes/one_node.gd`

```gdscript
extends Node
func calculate_one(input: int) -> int:
	var total: int = input + 8
	if total > 10:
		total += 7
	return total * 3
```

## formatting/check

Verify native formatting without writing source

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/formatting--check.json).

### Passing

`src/shared/queries/value_query.gd`

```gdscript
extends RefCounted


static func get_value() -> int:
	return 1
```

### Failing

`src/shared/queries/value_query.gd`

```gdscript
extends RefCounted
static func get_value()->int:
    return 1
```

## gdstyle/format/blank-lines

Collapse 3+ blank lines to 2 between top-level members

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/boolean-operators

Use 'and'/'or'/'not' instead of '&&'/'||'/'!'

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/brace-spacing

Single-line dictionary literals need a space after '{' and before '}'

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/call-paren-spacing

No space between a callee and its opening '('

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/colon-spacing

No space before ':' and one space after (except `:=` and end of line)

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/comma-spacing

No space before ',' and one space after (except newline / closing bracket)

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/comment-spacing

Comments must have a space after #

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/double-quotes

Prefer double quotes for strings

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/enum-one-per-line

Each enum member on its own line

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/float-literal-zeros

Float literals need leading/trailing zeros

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/large-number-underscores

Large numbers (integer part >= large_number_threshold, default 10_000) should use underscores

Default: **error**. Levels: off, warn, error.

Options: `large_number_threshold`.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/max-line-length

Lines must not exceed max length (default: 100)

Default: **error**. Levels: off, warn, error.

Options: `max_line_length`.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/no-tabs-as-spaces

Use consistent indentation (tabs by default)

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/no-unnecessary-parens

No unnecessary parentheses in if/while conditions

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/number-literals

Number literals must follow formatting rules

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/one-statement-per-line

One statement per line (no semicolons)

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/operator-spacing

One space around binary operators

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/trailing-comma

Trailing comma on last item of multi-line collections whose closing bracket starts its own line

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/trailing-newline

Files must end with a newline

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/format/trailing-whitespace

No trailing whitespace

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/naming/class-name-pascal-case

Class names must use PascalCase

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/naming/constant-name-screaming-case

Constants must use SCREAMING_SNAKE_CASE (or PascalCase for preloads)

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/naming/enum-member-screaming-case

Enum members must use SCREAMING_SNAKE_CASE

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/naming/enum-name-pascal-case

Enum names must use PascalCase

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/naming/file-name-snake-case

File names must use snake_case

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/naming/function-name-snake-case

Function names must use snake_case

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/naming/node-name-pascal-case

$NodePath references should use PascalCase

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/naming/private-underscore-prefix

Private members with _ should not have @export

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/naming/signal-name-snake-case

Signal names must use snake_case

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/naming/signal-past-tense

Signal names should use past tense

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/naming/variable-name-snake-case

Variable names must use snake_case

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/order/class-member-order

Class members must follow canonical ordering

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/allocation-in-loop

Flag .new() allocations inside loops

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/await-in-loop

Flag 'await' inside a for/while loop

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/duplicate-dict-key

Flag duplicate keys in dictionary literals

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/duplicated-load

Flag the same path passed to load()/preload() twice

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/empty-function

Flag functions whose body is only 'pass' (advisory, opt-in)

Default: **off**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/max-branches

Functions must not have too many branches

Default: **error**. Levels: off, warn, error.

Options: `max_branches`.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/max-class-variables

Classes must not have too many member variables

Default: **error**. Levels: off, warn, error.

Options: `max_class_variables`.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/max-file-length

Files must not exceed max length (default: 1000 lines)

Default: **error**. Levels: off, warn, error.

Options: `max_file_length`.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/max-function-length

Functions must not exceed max length (default: 50 lines)

Default: **error**. Levels: off, warn, error.

Options: `max_function_length`.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/max-inner-classes

Files must not have too many inner classes

Default: **error**. Levels: off, warn, error.

Options: `max_inner_classes`.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/max-local-variables

Functions must not have too many local variables

Default: **error**. Levels: off, warn, error.

Options: `max_local_variables`.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/max-nesting-depth

Functions must not nest blocks too deeply

Default: **error**. Levels: off, warn, error.

Options: `max_nesting_depth`.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/max-parameters

Functions must not have too many parameters (default: 5)

Default: **error**. Levels: off, warn, error.

Options: `max_parameters`.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/max-public-methods

Classes must not have too many public methods

Default: **error**. Levels: off, warn, error.

Options: `max_public_methods`.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/max-returns

Functions must not have too many return statements

Default: **error**. Levels: off, warn, error.

Options: `max_returns`.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/no-debug-print

Flag leftover print()/print_debug() calls (advisory, opt-in)

Default: **off**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/no-else-return

Drop the 'else' when the 'if' branch returns

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/no-self-assign

Flag assignments of a variable to itself

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/process-get-node

Flag $/get_node() lookups inside _process/_physics_process

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/self-comparison

Flag comparisons of a value with itself

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/type-hint

Declarations should have explicit type hints (advisory, opt-in)

Default: **error**. Levels: off, warn, error.

[Complete executable fixture](../examples/rules/gdstyle--quality--type-hint.json).

### Passing

`src/shared/queries/value_query.gd`

```gdscript
extends RefCounted
static func get_value() -> int:
	return 1
```

### Failing

`src/shared/queries/value_query.gd`

```gdscript
extends RefCounted
static func get_value():
	return 1
```

## gdstyle/quality/unnecessary-pass

'pass' alongside other statements is redundant

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/quality/unreachable-code

Flag code after return/break/continue

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/syntax/lex-error

Report lexer errors (unterminated strings, invalid numbers, bad characters)

Default: **error**. Input integrity: always an error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).

## gdstyle/syntax/reserved-identifier

Reserved words (namespace, trait, yield, ...) can't be used as names

Default: **error**. Levels: off, warn, error.

[Upstream rule reference](https://github.com/atelico/gdstyle/tree/v0.3.1).
