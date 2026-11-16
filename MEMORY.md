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
- The Unix installer lives at `script/install.sh`. Release `v0.1.3` includes native Windows x86-64 and ARM64 ZIPs for `script/install.ps1`. Windows tasks use Command Prompt syntax and require a local-drive project directory. Both installers verify the release checksum before replacing Lazel. [Windows OpenSpec change](openspec/changes/add-native-windows-support/proposal.md) · [v0.1.3 release](https://github.com/rohan-prasen/lazel/releases/tag/v0.1.3)

## Verification and next work

- On Apple Silicon, formatting, Clippy, the locked build, all 13 integration tests, and the mixed-language example passed. The live installer installed `v0.1.1` into a temporary home directory and the installed binary ran the example. A local installer test confirmed that a corrupt archive leaves the previous executable intact.
- All four release targets passed their CI binary smoke checks. The published release contained all four archives and their four checksum files. [Release workflow run](https://github.com/rohan-prasen/lazel/actions/runs/43711826812).
- The next milestone is opt-in local caching with declared inputs, outputs, and environment. The acceptance case is a repeat build that skips eligible work, restores missing outputs, and invalidates affected tasks when an input or prerequisite changes. Bounded parallel execution follows. [Approved roadmap](docs/superpowers/specs/2026-11-15-lazel-design.md).

## 2026-11-16 — v0.1.2 release failure

- **Failure.** Release eligibility passed for tag `v0.1.2` at `53bbc8a`. Linux and macOS checks passed. On `windows-2025`, `cargo clippy --locked --all-targets -- -D warnings` stopped with `E0658` in `src/workspace.rs` while compiling the binary and its test target. The Windows ARM64 job was canceled by matrix fail-fast. All six packaging jobs and publication were skipped, so this run produced no Windows release assets. [Failed release run](https://github.com/rohan-prasen/lazel/actions/runs/43806336427)
- **Root cause.** The Windows-only `cmd_directory` helper used `OsStr::slice_encoded_bytes` to remove the four-character `\\?\` prefix from a canonical local drive path. Rust 1.100.0 still gates this method behind `os_str_slice`. The local macOS checks could not catch the error because `#[cfg(windows)]` excludes the helper from compilation there. [Rust tracking issue](https://github.com/rust-lang/rust/issues/118485)
- **Fix.** The helper now removes the prefix using the stable Windows `OsStrExt::encode_wide` and `OsStringExt::from_wide` APIs. The UTF-16 round trip preserves the original path code units. The helper still verifies that the ordinary drive path resolves to the same directory before passing it to `cmd.exe`. The Windows CLI fixture now uses a project path with both a space and a non-ASCII character. [Rust Windows string extensions](https://doc.rust-lang.org/std/os/windows/ffi/trait.OsStrExt.html)
- **Verification.** The local macOS run passed formatting, Clippy, the locked build, 13 CLI tests, six release eligibility tests, both OpenSpec changes, and the mixed Rust/Python example. Hosted Rust CI for fix commit `c0895b2` passed on Linux, macOS, Windows x86-64, and Windows ARM64. The logs confirm native MSVC Rust 1.100.0 and a passing `runs_quoted_cmd_recipes_in_project_directory_and_stops_on_failure` test on each Windows architecture. [Passing Rust CI run](https://github.com/rohan-prasen/lazel/actions/runs/43806886710)
- **Pre-release checks and recovery.** Rust CI can now be dispatched on a branch before tagging. Matrix jobs finish independently so a failure on one platform still yields results for the other three. `v0.1.2` points to the original source, and rerunning its workflow checks the same commit. A new release tag is needed to verify all six packages and installer fixtures and publish the corrected binaries. [GitHub Actions rerun behavior](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/re-run-workflows-and-jobs)
- **Published fix.** `v0.1.3` passed all four CI jobs and all six packaging jobs, including installer fixtures on Windows PowerShell 5.1 and PowerShell 7 for both Windows targets. Downloads of all six published archives matched their checksums, and both Windows PE headers matched their advertised architectures. The live Bash installer returned `lazel 0.1.3` on Apple Silicon. The live Windows bootstrap remains unverified. [Release workflow](https://github.com/rohan-prasen/lazel/actions/runs/43807283695) · [Published assets](https://github.com/rohan-prasen/lazel/releases/tag/v0.1.3)

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
- 2026-11-16 — Bumped the crate and lockfile to `0.1.2` for the first Windows release.
- 2026-11-16 — Replaced the unstable Windows path API, added manual Rust CI with independent matrix jobs, and documented the release failure and recovery workflow.
- 2026-11-16 — Recorded the passing four-platform Rust CI run and both native Windows regression test results.
- 2026-11-16 — Prepared `v0.1.3` with the verified Windows fix while preserving the earlier `v0.1.2` tag.
- 2026-11-16 — Switched the README's PowerShell bootstrap to the requested one-line `irm | iex` command.
- 2026-11-16 — Recorded the successful `v0.1.3` release, checksums, Windows architecture checks, and live macOS install.
- 2026-11-16 — Replaced the generic Zed template with a Lazel dev container for stable Rust and Python, and added an opt-in firewall helper.
- 2026-11-16 — Linked the dev container ignore file to the root `.gitignore` and made the firewall helper executable.
- 2026-11-16 — Verified the dev container on Docker Desktop for Linux ARM64 with formatting, Clippy, 13 CLI tests, a locked build, the Rust/Python example, and two successful applications of the opt-in firewall in a disposable container.
