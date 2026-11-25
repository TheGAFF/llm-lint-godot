# Godot llm-lint

An opinionated linter for Godot 4 projects. It checks feature ownership, GDScript conventions, scene and resource references, maintained Markdown, and duplicate code. It uses a pinned `gdstyle` for GDScript linting and formatting checks.

The CLI runs locally and requires no model service or API key. Pin the version in CI and review the release notes before upgrading. The project is pre-1.0, and new rules can surface findings in existing projects.

## Install

Download the archive and its matching `.sha256` file from the [v0.6.0 GitHub release](https://github.com/TheGAFF/llm-lint-godot/releases/tag/v0.6.0). Choose Windows x64, Linux x64, macOS Intel, or macOS Apple Silicon. Verify the archive before extracting it.

Run these commands from the download directory in Windows PowerShell.

```powershell
$archive = 'godot-llm-lint-v0.6.0-x86_64-pc-windows-msvc.zip'
$expected = ((Get-Content "$archive.sha256" -Raw).Trim() -split '\s+')[0]
if ((Get-FileHash $archive -Algorithm SHA256).Hash -ine $expected) {
    throw 'Archive checksum mismatch'
}
Expand-Archive $archive
```

On Linux, run these commands with the archive you downloaded.

```sh
archive=godot-llm-lint-v0.6.0-x86_64-unknown-linux-gnu.tar.gz
sha256sum -c "$archive.sha256" && tar -xzf "$archive"
```

On macOS, use `shasum -a 256 -c "$archive.sha256"` with the archive filename for your Mac. The release matrix uses Windows Server 2025 x64, Ubuntu 22.04 x64, and macOS 15 on Intel and Apple Silicon.

Put the extracted executable on your PATH and run `godot-llm-lint --version`. The executables are unsigned. Verify the archive's GitHub build provenance with `gh attestation verify ARCHIVE --repo TheGAFF/llm-lint-godot`. If your OS requires a signed executable, use the tagged source install below.

You can also build from the release tag on any of these platforms. Install Rust 1.99.0 with rustup and have a C compiler available for the bundled parsers.

```sh
rustup toolchain install 1.99.0
cargo +1.99.0 install --git https://github.com/TheGAFF/llm-lint-godot.git --tag v0.6.0 --locked
godot-llm-lint --version
```

## Check a Godot project

Run these commands from the directory containing `project.godot`.

```sh
godot-llm-lint init
godot-llm-lint check
```

The default authored root is `src`, with code owned by `src/features/<feature>`, `src/shared`, or `src/app`. For a project using another root, use `init --source-root <directory>`. The root must exist before you initialize it.

Try the commands from the [minimal example project](examples/quickstart/) for a working layout.

`init` creates `llm-lint.godot.toml` and generates `gdstyle.toml`. It uses `curl` and `tar` to provision the pinned `gdstyle` 0.8.0 release in a per-user cache. Commit both configuration files. The [usage guide](docs/usage.md#native-tool-setup-and-trust) covers upgrades, offline provisioning, and CI.

Edit `llm-lint.godot.toml` for configuration changes and run `godot-llm-lint sync` to refresh `gdstyle.toml`. Checks are read-only by default.

Use [opt-in lint history](docs/usage.md#lint-history) to record full checks locally and compare results by rule, model, and baseline run.

On a build from this checkout, run [`godot-llm-lint unused --kind asset`](docs/usage.md#find-unused-content) to review potentially unused assets in size order. Run `godot-llm-lint unused` to include unreachable Godot files and unreferenced GDScript declarations.

## Before and after examples

These examples show manual fixes. **Opt-in** rules are disabled by default. The rule names omit the leading `architecture/` or `gdstyle/` prefix. Follow a rule link for details and executable fixtures where available.

In the tables, `…` marks an omitted path prefix, `↵` a newline, `⇥` a tab, and `·` a space.

### Ownership and file layout

| Rule(s) | Before | After |
| --- | --- | --- |
| [feature-layout](docs/rules.md#architecturefeature-layout) | `scripts/run_service.gd` | `src/features/run/services/run_service.gd` |
| [file-name](docs/rules.md#architecturefile-name) | `src/features/Run Mode/nodes/RunNode.gd` | `src/features/run_mode/nodes/run_node.gd` |
| [role-placement](docs/rules.md#architecturerole-placement) | `src/features/run/run_service.gd` | `src/features/run/services/run_service.gd` |
| [role-filename](docs/rules.md#architecturerole-filename) | `…/queries/run.gd` | `…/queries/run_query.gd` |
| [class-name](docs/rules.md#architectureclass-name) | `run_model.gd` declares `class_name EverythingManager` | `class_name RunModel` |
| [class-unique](docs/rules.md#architectureclass-unique) | `hero_node.gd` and `enemy_node.gd` both declare `Player` | `class_name HeroNode` / `class_name EnemyNode` |
| [role-base](docs/rules.md#architecturerole-base) | `…/models/run_model.gd`: `extends Node` | `extends RefCounted` |
| [role-layout-conflict](docs/rules.md#architecturerole-layout-conflict) | `…/run_queries.gd` alongside `…/queries/run_query.gd` | Move collection functions into `…/queries/*_query.gd`; remove the collection |
| [collection-size](docs/rules.md#architecturecollection-size) | `…/run_queries.gd`: 1,200 code lines (default limit: 300) | Split into focused `…/queries/*_query.gd` files |
| [testing-name](docs/rules.md#architecturetesting-name) | `…/testing/save.gd` | `…/testing/test_save.gd` |
| [asset-kind](docs/rules.md#architectureasset-kind) | `…/assets/blip.ogg`: music or sound effect? | `…/assets/sound_effects/blip_sound_effect.ogg` |
| [asset-collision](docs/rules.md#architectureasset-collision) | `music_battle.ogg` suggests `battle_music.ogg`, which already exists | Give the incoming track a distinct name: `boss_battle_music.ogg` |
| [companion-files](docs/rules.md#architecturecompanion-files) | `player_node.gd` lacks its configured required `.gd.uid` companion | `player_node.gd` + `player_node.gd.uid` (pairs are project-configured) |

### Dependencies and resources

| Rule(s) | Before | After |
| --- | --- | --- |
| [dependency-direction](docs/rules.md#architecturedependency-direction) | `src/shared/queries/run_query.gd` loads `src/features/run/models/run_model.gd` | Move the caller to `src/features/run/queries/run_query.gd` |
| [reference](docs/rules.md#architecturereference) | `load("res://…/Run_Model.gd")`, but only `run_model.gd` exists | `load("res://…/run_model.gd")` with exact casing |
| [dynamic-load](docs/rules.md#architecturedynamic-load) | `load(path)` with no declared bounds | `load("res://src/features/content/resources/world_resource.tres")`; or declare function-specific roots and a reason |
| [dynamic-load-bound](docs/rules.md#architecturedynamic-load-bound) | Declared `roots = ["user://saves"]`, but `load("user://other/run_resource.tres")` | `load("user://saves/run_resource.tres")` |
| [dynamic-load-stale](docs/rules.md#architecturedynamic-load-stale) | `[[dynamic_loads]]` still names deleted `load_run` | Remove that unused declaration |
| [resource-id-name](docs/rules.md#architectureresource-id-name) **opt-in** | `fireball_resource.tres`: `id = &"icebolt"` | `id = &"fireball"` |
| [resource-id-type](docs/rules.md#architectureresource-id-type) **opt-in** | `@export var id: String = "fireball"` | `@export var id: StringName = &"fireball"` |
| [stringname-literal](docs/rules.md#architecturestringname-literal) **opt-in** | `var id: StringName = "fireball"` | `var id: StringName = &"fireball"` |

### Encapsulation and utilities

| Rule(s) | Before | After |
| --- | --- | --- |
| [function-role](docs/rules.md#architecturefunction-role) | `static func validate_run() -> bool:` in `…/queries/run_query.gd` | Move it to `…/validators/run_validator.gd` |
| [utility-state](docs/rules.md#architectureutility-state) | A query owns `var count: int = 0` and instance `func get_count()` | Put state in `run_model.gd`; query via `static func get_count(run: RunModel) -> int:` |
| [member-underscore-required](docs/rules.md#architecturemember-underscore-required) | File-local `var cooldown: float` and `func recalculate_path()` look public | `var _cooldown: float` and `func _recalculate_path()` |
| [private-member-access](docs/rules.md#architectureprivate-member-access) | Another script calls `spawner._recalculate_path()` | Expose `recalculate_path()`; call `spawner.recalculate_path()` |
| [local-variable-snake-case](docs/rules.md#architecturelocal-variable-snake-case) | Function-local `var _SpawnCount: int = 99` | `var spawn_count: int = 99` |
| [mapper-return-type](docs/rules.md#architecturemapper-return-type) | `func map_user_dto() -> Array[UserDto]:` | `func map_user_dtos() -> Array[UserDto]:` |
| [mapper-output-declaration](docs/rules.md#architecturemapper-output-declaration) | `func map_user_dto() -> UserDto:`, but `UserDto` is undeclared | Declare `class UserDto:`<br>`⇥extends RefCounted`, or resolve an existing class |

### Documentation

| Rule(s) | Before | After |
| --- | --- | --- |
| [document-role](docs/rules.md#architecturedocument-role) | `…/docs/brain_dump.md` | `…/docs/guides/run_guide.md` |
| [document-collision](docs/rules.md#architecturedocument-collision) | `…/docs/save_guide.md` suggests `…/docs/guides/save_guide.md`, which already exists | Preserve both topics: name the incoming document `…/docs/guides/autosave_guide.md` |
| [document-metadata](docs/rules.md#architecturedocument-metadata) | TOML front matter: `reviewed = "9999-01-01"` | An actual review date: `reviewed = "2026-11-10"` |
| [document-blueprint](docs/rules.md#architecturedocument-blueprint) | Decision section: `## Explanation` | `## Rationale` in the role's required section order |
| [document-structure](docs/rules.md#architecturedocument-structure) | Guide's `Steps`: `- Resume the run.` | `1. Resume the run.` (ordered steps) |
| [document-sources](docs/rules.md#architecturedocument-sources) | Key Files declares `../outside.txt` | Declare `src/features/run/models/run_model.gd` with its purpose |
| [document-stale](docs/rules.md#architecturedocument-stale) | `run_model.gd` changed; its guide still carries the old `sources_hash` | Review sources/prose, then `godot-llm-lint docs review …/run_guide.md --reviewed 2026-11-10` |
| [document-size](docs/rules.md#architecturedocument-size) | A 900-line, 9,000-word guide | Split focused `*_guide.md` / `*_reference.md` topics; each ≤150 lines / 1,500 words by default |

### Duplication and suppressions

| Rule(s) | Before | After |
| --- | --- | --- |
| [duplicates/exact](docs/rules.md#duplicatesexact), [duplicates/renamed](docs/rules.md#duplicatesrenamed) | Two 20-line / 200-token damage bodies, pasted verbatim or with only identifiers renamed | Extract `_apply_damage(target, amount)`; both callers reuse it |
| [duplicates/similar](docs/rules.md#duplicatessimilar) | Large `apply_burn()` / `apply_poison()` bodies with nearly identical AST structure | Review behavior; extract common steps into `_apply_status()` and retain distinct conditions |
| [comment-suppression](docs/rules.md#architecturecomment-suppression) | `# llm-lint-ignore *`<br>`@export var id: String = "external"` | With the ID-type rule enabled:<br>`# llm-lint-ignore architecture/resource-id-type: External schema requires String.`<br>`@export var id: String = "external"` |
| [native-suppression](docs/rules.md#architecturenative-suppression) | `# gdstyle:ignore-file` hides the entire script | Inside a spawn loop:<br>`# reason: Each spawn needs its own node.`<br>`var enemy: Node = Node.new() # gdstyle:ignore=quality/allocation-in-loop` |
| [exception-stale](docs/rules.md#architectureexception-stale) | `[[exceptions]]` still exempts a corrected `architecture/class-name` violation | Remove that exact unused exception |

### Naming and syntax

| Rule(s) | Before | After |
| --- | --- | --- |
| [naming/class-name-pascal-case](docs/rules.md#gdstylenamingclass-name-pascal-case) | `class_name enemy_boss` | `class_name EnemyBoss` |
| [naming/file-name-snake-case](docs/rules.md#gdstylenamingfile-name-snake-case) | `BossAI.gd` | `boss_ai.gd` |
| [naming/function-name-snake-case](docs/rules.md#gdstylenamingfunction-name-snake-case) | `func SpawnENEMY() -> void:` | `func spawn_enemy() -> void:` |
| [naming/variable-name-snake-case](docs/rules.md#gdstylenamingvariable-name-snake-case) | `var MaxHP: int = 999` | `var max_hp: int = 999` |
| [naming/constant-name-screaming-case](docs/rules.md#gdstylenamingconstant-name-screaming-case) | `const maxHP: int = 999` | `const MAX_HP: int = 999` |
| [naming/enum-name-pascal-case](docs/rules.md#gdstylenamingenum-name-pascal-case), [naming/enum-member-screaming-case](docs/rules.md#gdstylenamingenum-member-screaming-case) | `enum gameState { idle, running }` | `enum GameState { IDLE, RUNNING }` |
| [naming/signal-name-snake-case](docs/rules.md#gdstylenamingsignal-name-snake-case), [naming/signal-past-tense](docs/rules.md#gdstylenamingsignal-past-tense) | `signal Player_die` | `signal player_died` |
| [naming/private-underscore-prefix](docs/rules.md#gdstylenamingprivate-underscore-prefix) | `@export var _speed: float = 20.0` | `@export var speed: float = 20.0` |
| [naming/node-name-pascal-case](docs/rules.md#gdstylenamingnode-name-pascal-case) | `$world/bad_guy` | Rename scene nodes and reference: `$World/BadGuy` |
| [order/class-member-order](docs/rules.md#gdstyleorderclass-member-order) | `func _ready()` appears before `signal died` | Declare `signal died` before the methods |
| [architecture/syntax](docs/rules.md#architecturesyntax), [syntax/lex-error](docs/rules.md#gdstylesyntaxlex-error) | `var title: String = "boss` (unterminated string) | `var title: String = "boss"` |
| [syntax/reserved-identifier](docs/rules.md#gdstylesyntaxreserved-identifier) | `var namespace: int = 1` | `var namespace_id: int = 1` |

### Code quality

Numeric limits are configurable. Large bodies below are summarized rather than printed in full.

| Rule(s) | Before | After |
| --- | --- | --- |
| [quality/type-hint](docs/rules.md#gdstylequalitytype-hint) | `var hp = 100`; `func damage(enemy):` | `var hp: int = 100`; `func damage(enemy: EnemyNode) -> void:` |
| [quality/allocation-in-loop](docs/rules.md#gdstylequalityallocation-in-loop) | `RegEx.new()` on every loop iteration | Create `var regex: RegEx = RegEx.new()` before the loop; reuse it |
| [quality/await-in-loop](docs/rules.md#gdstylequalityawait-in-loop) | `await _animate(enemy)` inside every loop iteration | Start animations together; `await animations_finished` outside the loop |
| [quality/process-get-node](docs/rules.md#gdstylequalityprocess-get-node) | `$HUD/Score.text = str(score)` inside `_process()` | Cache `@onready var _score_label: Label = $HUD/Score`; update `_score_label.text` |
| [quality/duplicate-dict-key](docs/rules.md#gdstylequalityduplicate-dict-key) | `{ "hp": 100, "hp": 0 }` | `{ "hp": 100, "shield": 0 }` |
| [quality/duplicated-load](docs/rules.md#gdstylequalityduplicated-load) | Two `preload("res://…/enemy_scene.tscn")` calls in one script | One `const ENEMY_SCENE = preload("res://…/enemy_scene.tscn")`; reuse it |
| [quality/no-self-assign](docs/rules.md#gdstylequalityno-self-assign) | `hp = hp` | Remove the no-op assignment |
| [quality/self-comparison](docs/rules.md#gdstylequalityself-comparison) | `if hp == hp:` | Compare the intended values: `if hp == max_hp:` |
| [quality/unnecessary-pass](docs/rules.md#gdstylequalityunnecessary-pass) | `hp -= 1`<br>`pass` | `hp -= 1` |
| [quality/unreachable-code](docs/rules.md#gdstylequalityunreachable-code) | `return hp`<br>`save_run()` | `save_run()`<br>`return hp` |
| [quality/no-else-return](docs/rules.md#gdstylequalityno-else-return) | `if dead:↵⇥return 0↵else:↵⇥return hp` | `if dead:↵⇥return 0↵return hp` |
| [quality/empty-function](docs/rules.md#gdstylequalityempty-function) **opt-in** | `func save_run() -> void:↵⇥pass` | Implement `save_run()` or remove the unused stub |
| [quality/no-debug-print](docs/rules.md#gdstylequalityno-debug-print) **opt-in** | `print_debug("HERE!!!", player, enemies)` left in production | Remove the debugging call |
| [quality/max-parameters](docs/rules.md#gdstylequalitymax-parameters) | `func spawn(x, y, hp, speed, damage, armor, tier, color):` (8 parameters; default limit: 5) | `func spawn(spec: SpawnModel) -> void:` |
| [quality/max-function-length](docs/rules.md#gdstylequalitymax-function-length) | `tick()`: 500 lines (default limit: 50) | Extract focused `_move()`, `_target()`, `_attack()` helpers, each below the limit |
| [quality/max-file-length](docs/rules.md#gdstylequalitymax-file-length) | `everything_node.gd`: 10,000 lines (default limit: 1,000) | Split into `player_node.gd`, `combat_service.gd` and other focused scripts below the limit |
| [quality/max-nesting-depth](docs/rules.md#gdstylequalitymax-nesting-depth) | 20 nested `if` / `for` blocks in one function | Guard clauses and extracted helpers; at most 2 nested blocks here |
| [quality/max-branches](docs/rules.md#gdstylequalitymax-branches) | `handle_action()`: 200 `if` / `elif` branches | Dispatch through `_handlers[action].call()` or focused handlers |
| [quality/max-returns](docs/rules.md#gdstylequalitymax-returns) | `calculate_damage()`: 200 return statements | Separate calculation steps; one final `return damage` |
| [quality/max-local-variables](docs/rules.md#gdstylequalitymax-local-variables) | `simulate()`: 200 locals for unrelated tasks | Extract focused functions, each using only a few locals |
| [quality/max-class-variables](docs/rules.md#gdstylequalitymax-class-variables) | `GodNode`: 200 member variables | Move distinct state into focused `RunModel`, `InventoryModel`, etc. |
| [quality/max-public-methods](docs/rules.md#gdstylequalitymax-public-methods) | `GodService` exposes 200 public methods | Split services by responsibility; keep file-local helpers private |
| [quality/max-inner-classes](docs/rules.md#gdstylequalitymax-inner-classes) | `everything.gd` nests 200 unrelated classes | Extract `*_model.gd` files; retain only closely related inner classes |

### Formatting

`formatting/check` verifies the pinned formatter's output; individual `format/*` rules report specific issues. These examples use the default tab indentation.

| Rule(s) | Before | After |
| --- | --- | --- |
| [formatting/check](docs/rules.md#formattingcheck) | `var hp:int=100` | `var hp: int = 100` |
| [format/boolean-operators](docs/rules.md#gdstyleformatboolean-operators) | `if !dead && armed:` | `if not dead and armed:` |
| [format/operator-spacing](docs/rules.md#gdstyleformatoperator-spacing) | `hp=hp+damage*2` | `hp = hp + damage * 2` |
| [format/brace-spacing](docs/rules.md#gdstyleformatbrace-spacing) | `{"hp": 100}` | `{ "hp": 100 }` |
| [format/call-paren-spacing](docs/rules.md#gdstyleformatcall-paren-spacing) | `spawn (enemy)` | `spawn(enemy)` |
| [format/colon-spacing](docs/rules.md#gdstyleformatcolon-spacing) | `var hp :int = 100` | `var hp: int = 100` |
| [format/comma-spacing](docs/rules.md#gdstyleformatcomma-spacing) | `Vector2(1 ,2)` | `Vector2(1, 2)` |
| [format/comment-spacing](docs/rules.md#gdstyleformatcomment-spacing) | `#0 means inactive.` | `# 0 means inactive.` |
| [format/double-quotes](docs/rules.md#gdstyleformatdouble-quotes) | `var title: String = 'Boss'` | `var title: String = "Boss"` |
| [format/enum-one-per-line](docs/rules.md#gdstyleformatenum-one-per-line) | `enum State { IDLE, RUNNING }` | `enum State {↵⇥IDLE,↵⇥RUNNING,↵}` |
| [format/extra-spacing](docs/rules.md#gdstyleformatextra-spacing) | `print( value )` | `print(value)` |
| [format/number-literals](docs/rules.md#gdstyleformatnumber-literals) | `0xFF` | `0xff` |
| [format/float-literal-zeros](docs/rules.md#gdstyleformatfloat-literal-zeros) | `var weight: float = 1.` | `var weight: float = 1.0` |
| [format/large-number-underscores](docs/rules.md#gdstyleformatlarge-number-underscores) | `var gold: int = 10000000` | `var gold: int = 10_000_000` |
| [format/one-statement-per-line](docs/rules.md#gdstyleformatone-statement-per-line) | `move(); attack(); save()` | `move()↵attack()↵save()` |
| [format/no-unnecessary-parens](docs/rules.md#gdstyleformatno-unnecessary-parens) | `if (alive):` | `if alive:` |
| [format/no-tabs-as-spaces](docs/rules.md#gdstyleformatno-tabs-as-spaces) | Four-space function-body indents with `use_tabs = true` | Tab indents |
| [format/blank-lines](docs/rules.md#gdstyleformatblank-lines) | Five blank lines between top-level methods | Two blank lines |
| [format/max-line-length](docs/rules.md#gdstyleformatmax-line-length) | One 400-character `spawn(...)` line (default limit: 100) | Parenthesized multiline call, each line ≤100 characters |
| [format/trailing-comma](docs/rules.md#gdstyleformattrailing-comma) | `[↵⇥1↵]` | `[↵⇥1,↵]` |
| [format/trailing-newline](docs/rules.md#gdstyleformattrailing-newline) | `return hp↵↵` at end of file | `return hp↵` |
| [format/trailing-whitespace](docs/rules.md#gdstyleformattrailing-whitespace) | `return hp···↵` | `return hp↵` |

The [usage guide](docs/usage.md) covers rule settings, file roles, Markdown, and duplicate review. See the [full rule reference](docs/rules.md) for details and the [v0.6.0 release notes](docs/releases/v0.6.0.md) for upgrade steps.

## Rust development

Run `cargo fmt --all -- --check` and `cargo clippy --locked --all-targets` from the repository root. To run `cargo test --locked`, set `GDSTYLE_TEST_BINARY` to the platform executable whose SHA-256 is listed in the [pinned release manifest](policy/gdstyle-release.json).

Maintainers with a private sibling checkout of `llm-lint-rust` also run this check before committing Rust changes.

```sh
cargo +1.99.0 run --locked --manifest-path ../llm-lint-rust/Cargo.toml --bin cargo-llm-lint -- check
```

[MIT license](LICENSE) · [Third-party notices](THIRD_PARTY_NOTICES.txt)
