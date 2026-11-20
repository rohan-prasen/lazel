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
- 2026-11-19 — Added a CLI regression test for both self-update command names and the official installation path boundary.
- 2026-11-19 — Added native release discovery, verified downloads, and platform-specific replacement for the `update` and `upgrade` commands, and bumped the package to `0.1.4`.
- 2026-11-19 — Updated the Cargo lockfile for Lazel 0.1.4 and its native updater dependencies.
- 2026-11-19 — Documented updater behavior and release history, linked the changelog in generated release notes, and added updater test coverage to the six-target packaging workflow.
- 2026-11-19 — Locked a maintained ZIP release for the Windows updater.
- 2026-11-19 — Added offline updater fixtures for release selection, verification failures, concurrent updates, and replacing a running executable.
- 2026-11-19 — Formatted the updater implementation and its regression tests.
- 2026-11-19 — Matched Windows archive entries against the ZIP crate's exact filename bytes.
- 2026-11-19 — Isolated failure fixtures and added a regression test for duplicate Windows archive entries.
- 2026-11-19 — Rejected duplicate ZIP paths before extracting the Windows executable and shared the release build flags between build and test steps.
- 2026-11-19 — Selected static Windows build flags from the release matrix target and clarified errors reading the executable entry from a ZIP.
- 2026-11-19 — Formatted the ZIP validation and isolated updater fixtures after review.
- 2026-11-19 — Fixed Windows ZIP extraction to decompress the exact matched executable and included exit status in version-check failures.
- 2026-11-19 — Kept the musl C compiler for Rustls dependencies and restored Rust's default linker after the x86-64 musl linker override produced a crashing dynamic executable.
- 2026-11-19 — Moved the updater unit tests under `tests/`, added an ignore rule for the local release eligibility test, and documented the test layout.
- 2026-11-19 — Removed the release eligibility test from Git tracking while preserving its local copy and verified the cleanup with the local Rust and Python checks.
- 2026-11-19 — Renamed the updater CLI test to `tests/update_cli.rs` to distinguish it from the updater unit tests.
- 2026-11-19 — Verified that the renamed CLI test and the updater unit tests both run in the locked test suite.
- 2026-11-20 — Added a root roadmap for Turborepo feature parity, starting with opt-in local caching and bounded parallel execution.
- 2026-11-20 — Created the Lazel project in Linear's Solo team for engineering work.
- 2026-11-20 — Added the four missing default triage labels to Lazel's GitHub issue tracker.
- 2026-11-20 — Added the five default triage labels to Linear's Solo team.
- 2026-11-20 — Configured the Matt Pocock engineering skills for GitHub Issues, Linear, shared triage labels, and single-context domain docs.
- 2026-11-20 — Scaffolded the OpenSpec change for opt-in local task caching.
- 2026-11-20 — Wrote the proposal for opt-in local task caching and its CLI and compatibility scope.
- 2026-11-20 — Specified cache configuration, invalidation, output recovery, concurrency, and ordered execution behavior.
- 2026-11-20 — Documented the local cache design for environment isolation, deterministic keys, coordinated writes, and verified output rollback.
- 2026-11-20 — Added the implementation checklist and cross-platform verification steps for local task caching.
- 2026-11-20 — Added cross-platform cache regression tests before implementing the runner changes.
- 2026-11-20 — Added strict cache configuration and workspace-wide output ownership preflight.
- 2026-11-20 — Exposed the shared filesystem path comparison for cache preflight.
- 2026-11-20 — Locked the glob matcher used for cache input declarations.
- 2026-11-20 — Selected the locally available glob 0.3.3 for locked offline verification.
- 2026-11-20 — Verified cache preflight and all thirteen legacy Unix runner tests.
- 2026-11-20 — Added the cache bypass flag and isolated task environments through the shared shell launcher.
- 2026-11-20 — Verified that an explicit cache bypass preserves shell output, environment, and exit status locally.
- 2026-11-20 — Added safe project input expansion and versioned fingerprints for task inputs, environment, and prerequisite results.
- 2026-11-20 — Fixed the fingerprint test to accept independent environment snapshots.
- 2026-11-20 — Verified fingerprint changes for task inputs, declared toolchains, and prerequisite results.
- 2026-11-20 — Implemented locked cache storage, verified staging, and rollback when an output restore fails.
- 2026-11-20 — Wired cache hits, misses, bypasses, and prerequisite fingerprints into the sequential runner.
- 2026-11-20 — Added regression coverage for corrupt artifacts, unsafe outputs, changing inputs, and dependency restoration.
- 2026-11-20 — Verified cache publication, workspace serialization, empty results, and skipped writes after failure or input changes.
- 2026-11-20 — Verified corrupt-entry detection, unsafe output protection, and staged restore rollback.
- 2026-11-20 — Verified cache status, dependency restoration, and explicit bypass through CLI fixtures.
- 2026-11-20 — Documented opt-in caching and added cold, warm, restore, and invalidation checks to the Rust/Python example.
- 2026-11-20 — Recorded the project in restore recovery manifests and made the example verifier tolerate toolchain warnings.
- 2026-11-20 — Formatted the cache implementation and regression tests.
- 2026-11-20 — Split portable and Windows-specific path validation to satisfy strict Clippy checks.
- 2026-11-20 — Applied Rust formatting to the revised path checks.
- 2026-11-20 — Passed formatting, strict Clippy, all 36 local tests, a locked build, and the Rust/Python cache example on Apple Silicon.
- 2026-11-20 — Confirmed native Windows x86-64 and ARM64 fixtures for cache restoration, concurrent locking, path conflicts, and shell quoting.
- 2026-11-20 — Recorded the passing four-platform Rust CI run for pull request #2 and completed the local caching checklist. [Rust CI run](https://github.com/rohan-prasen/lazel/actions/runs/44701753961)
- 2026-11-20 — Bumped the manifest and lockfile to 0.1.5 after merging opt-in local caching in pull request #2.
- 2026-11-20 — Verified that the cache fingerprint distinguishes missing and empty environment values.
