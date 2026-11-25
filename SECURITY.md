# Security policy

Security fixes target the latest published minor release. To report a
vulnerability, use
[GitHub's private reporting form](https://github.com/TheGAFF/llm-lint-godot/security/advisories/new).
Include the linter version, platform, command, and a minimal sanitized example.
Keep credentials and private project files out of reports.

The CLI analyzes repository inputs locally. Online initialization downloads a
pinned gdstyle archive and verifies its archive and executable against
embedded SHA-256 digests. Keep custom executables, cache settings, and digests
under the caller's control. Run checks for unfamiliar projects on isolated,
unprivileged runners with read-only tokens and no secrets.

Lint history is opt-in. Local history databases and exported reports can
contain model labels, paths, and line numbers. Review them before sharing.

## Pinned gdstyle reviews

The CI advisory scan covers the exact gdstyle 0.8.0 source lockfile. The
lockfile covers the CLI, tests, and Godot extension. The published CLI
builds gdstyle with its default features. Its runtime graph includes
crossbeam-epoch 0.9.18 through Rayon's worker queues.

- [RUSTSEC-2026-0204](https://rustsec.org/advisories/RUSTSEC-2026-0204)
  applies to the epoch pointer-formatting trait. The reviewed CLI
  passes lint work through Rayon, and the worker queues use epoch for
  storage and reclamation. Their formatting paths never invoke that trait.
  CI accepts this advisory only for the SHA-pinned gdstyle lockfile.
  Reassess it on a gdstyle upgrade and remove the exception when the
  upstream lockfile reaches crossbeam-epoch 0.9.20 or later.
- The lockfile also lists RustSec informational warnings for anyhow and
  rustix. Both are outside the normal and build dependency graph of
  the shipped gdstyle CLI.
