# Lazel repository memory

This snapshot records Lazel's development through 2026-11-15. Lazel (*lazy + Bazel*) is a Rust CLI that coordinates existing commands across projects in a monorepo, regardless of the languages or frameworks those projects use.

## Starting point

Work began at `d95e4b8`. The Rust 2024 package used Clap for argument parsing and `anyhow` for errors. `lazel run <task>` printed the selected task and a placeholder. Workspace discovery, configuration, dependency planning, and command execution were still to be implemented. The test directory contained only `.gitkeep`. OpenSpec, pre-commit, a dev container, and a Copilot setup workflow were already in place.

## How Lazel reached v0.1.1

1. **Defined the first milestone.** The project chose one root TOML file, shell command strings, sequential dependency execution, and support for macOS and Linux. A Python app that uses the output of a Rust generator became the acceptance example. The [approved design](docs/superpowers/specs/2026-11-15-lazel-design.md), [implementation plan](docs/superpowers/plans/2026-11-15-lazel-implementation.md), and [OpenSpec requirements](openspec/changes/v0-1-task-runner/specs/task-runner/spec.md) record the decisions.
2. **Implemented the runner at `fa4d28d`, tagged `v0.1.0`.** Added TOML loading with Serde, nearest-ancestor workspace discovery, preflight validation, and iterative depth-first dependency planning. The CLI runs selected tasks through `/bin/sh -c` in their project directories. Added 13 CLI integration tests, the [mixed-language example](examples/mixed-language/lazel.toml), and a macOS and Linux [Rust CI workflow](.github/workflows/rust-ci.yml).
3. **Published binaries at `935f980`, tagged `v0.1.1`.** Added the [root installer](install.sh), [README installation instructions](README.md#install), and a [tag-triggered release workflow](.github/workflows/release.yml). Preserved the original `v0.1.0` tag. The workflow builds and smoke-tests four targets, checks the binary version against the tag, and publishes an archive and SHA-256 checksum for each target. [GitHub release](https://github.com/rohan-prasen/lazel/releases/tag/v0.1.1) · [Successful release run](https://github.com/rohan-prasen/lazel/actions/runs/43711826812).

## Current behavior

- `lazel run <project>:<task>` finds the nearest `lazel.toml` by walking upwards, including across Git boundaries. Projects declare a relative directory inside the workspace. Tasks declare a shell command and optional fully qualified dependencies.
- Lazel validates the whole workspace before launching commands. It visits dependencies in declaration order, runs each shared prerequisite once, and aborts at the first failure. Shell commands inherit the caller's environment and standard streams. Lazel sends its own progress and errors to stderr.
- The latest published release at this checkpoint is `v0.1.1`. The installer selects a binary for macOS or Linux on x86-64 or ARM64, verifies its checksum, and installs it in `$HOME/.local/bin`. The Linux binaries target musl. [Latest release](https://github.com/rohan-prasen/lazel/releases/latest).

## Verification and next work

- On Apple Silicon, formatting, Clippy, the locked build, all 13 integration tests, and the mixed-language example passed. The live installer installed `v0.1.1` into a temporary home directory and the installed binary ran the example. A local installer test confirmed that a corrupt archive leaves the previous executable intact.
- All four release targets passed their CI binary smoke checks. The published release contained all four archives and their four checksum files. [Release workflow run](https://github.com/rohan-prasen/lazel/actions/runs/43711826812).
- The next milestone is opt-in local caching with declared inputs, outputs, and environment. The acceptance case is a repeat build that skips eligible work, restores missing outputs, and invalidates affected tasks when an input or prerequisite changes. Bounded parallel execution follows. [Approved roadmap](docs/superpowers/specs/2026-11-15-lazel-design.md).

## Change log

- 2026-11-15 — Added a rule in `AGENTS.md` to log every project-changing tool call or edit here as a short, dated entry.
