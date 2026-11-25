# Godot llm-lint quickstart

Copy this directory to a writable location and open a terminal beside
`project.godot`.

    godot-llm-lint init
    godot-llm-lint sync --check
    godot-llm-lint check

Remove the return type from `value_query.gd` and run
`godot-llm-lint check --json` to see the resulting type-hint diagnostic.

See the [usage guide](../../docs/usage.md) for existing projects and offline setup.
