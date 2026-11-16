# Lazel repository memory

This snapshot records Lazel's development through 2026-11-16. Lazel (*lazy + Bazel*) is a Rust CLI that coordinates existing commands across projects in a monorepo, regardless of the languages or frameworks those projects use.

## Starting point

Work began at `d95e4b8`. The Rust 2024 package used Clap for argument parsing and `anyhow` for errors. `lazel run <task>` printed the selected task and a placeholder. Workspace discovery, configuration, dependency planning, and command execution were still to be implemented. The test directory contained only `.gitkeep`. OpenSpec, pre-commit, a dev container, and a Copilot setup workflow were already in place.

## How Lazel reached v0.1.1

1. **Defined the first milestone.** The project chose one root TOML file, shell command strings, sequential dependency execution, and support for macOS and Linux. A Python app that uses the output of a Rust generator became the acceptance example. The [approved design](docs/superpowers/specs/2026-11-15-lazel-design.md), [implementation plan](docs/superpowers/plans/2026-11-15-lazel-implementation.md), and [OpenSpec requirements](openspec/changes/v0-1-task-runner/specs/task-runner/spec.md) record the decisions.
2. **Implemented the runner at `fa4d28d`, tagged `v0.1.0`.** Added TOML loading with Serde, nearest-ancestor workspace discovery, preflight validation, and iterative depth-first dependency planning. The CLI runs selected tasks through `/bin/sh -c` in their project directories. Added 13 CLI integration tests, the [mixed-language example](examples/mixed-language/lazel.toml), and a macOS and Linux [Rust CI workflow](.github/workflows/rust-ci.yml).
3. **Published binaries at `935f980`, tagged `v0.1.1`.** Added the [original root installer](https://github.com/rohan-prasen/lazel/blob/v0.1.1/install.sh), [README installation instructions](README.md#install), and a [tag-triggered release workflow](.github/workflows/release.yml). Preserved the original `v0.1.0` tag. The workflow built and smoke-tested four targets, checked the binary version against the tag, and published an archive and SHA-256 checksum for each target. [GitHub release](https://github.com/rohan-prasen/lazel/releases/tag/v0.1.1) · [Successful release run](https://github.com/rohan-prasen/lazel/actions/runs/43711826812).

## Current behavior

- `lazel run <project>:<task>` finds the nearest `lazel.toml` by walking upwards, including across Git boundaries. Projects declare a relative directory inside the workspace. Tasks declare a shell command and optional fully qualified dependencies.
- Lazel validates the whole workspace before launching commands. It visits dependencies in declaration order, runs each shared prerequisite once, and aborts at the first failure. Shell commands inherit the caller's environment and standard streams. Lazel sends its own progress and errors to stderr.
- The Unix installer now lives at `script/install.sh`. The next eligible release also builds native Windows x86-64 and ARM64 ZIPs for `script/install.ps1`. Windows tasks use Command Prompt syntax and require a local-drive project directory. Both installers verify the release checksum before replacing Lazel. [Windows OpenSpec change](openspec/changes/add-native-windows-support/proposal.md) · [Latest release](https://github.com/rohan-prasen/lazel/releases/latest).

## Verification and next work

- On Apple Silicon, formatting, Clippy, the locked build, all 13 integration tests, and the mixed-language example passed. The live installer installed `v0.1.1` into a temporary home directory and the installed binary ran the example. A local installer test confirmed that a corrupt archive leaves the previous executable intact.
- All four release targets passed their CI binary smoke checks. The published release contained all four archives and their four checksum files. [Release workflow run](https://github.com/rohan-prasen/lazel/actions/runs/43711826812).
- The next milestone is opt-in local caching with declared inputs, outputs, and environment. The acceptance case is a repeat build that skips eligible work, restores missing outputs, and invalidates affected tasks when an input or prerequisite changes. Bounded parallel execution follows. [Approved roadmap](docs/superpowers/specs/2026-11-15-lazel-design.md).

## Change log

- 2026-11-15 — Added a rule in `AGENTS.md` to log every project-changing tool call or edit here as a short, dated entry.
- 2026-11-15 — Polished the README introduction and added release, CI, license, and stars badges and a link to star the repository.
- 2026-11-15 — Updated the README badges to show release build status and link directly to the repository's Star button.
- 2026-11-15 — Moved Rust checks behind version tags and added a release eligibility check for a newer crate version and source or dependency changes.
- 2026-11-15 — Added release eligibility regression tests for skipped and qualifying releases.
- 2026-11-16 — Added native Windows task execution with Command Prompt and a regression test for quoting, project directories, and exit codes.
- 2026-11-16 — Moved the Unix installer into `script/` and added a PowerShell installer with checksum verification, upgrade protection, and PATH guidance.
- 2026-11-16 — Expanded CI and the release matrix to Windows x86-64 and ARM64, and added fixture-based installer checks for the packaged binaries.
- 2026-11-16 — Documented both installer commands, Windows task configuration, and the six-target release process.
- 2026-11-16 — Formatted the Rust changes and preserved executable permissions on the relocated Bash installer.
- 2026-11-16 — Fixed the PowerShell installer fixture so it resolves its archive independently of the installer scope.
- 2026-11-16 — Enabled TLS 1.2 for the Windows PowerShell bootstrap download in the README.
- 2026-11-16 — Scaffolded the OpenSpec change for native Windows support and release installers.
- 2026-11-16 — Recorded the Windows release, installer paths, and task-runner scope in the OpenSpec proposal.
- 2026-11-16 — Documented the Windows design and added OpenSpec requirements for native task execution, release assets, and verified installation.
- 2026-11-16 — Recorded completed Windows implementation tasks and the remaining hosted and live-release checks in OpenSpec.
