# Changelog

## Unreleased

- Select multiple tasks with `lazel run build lint` and narrow the requested
  targets with a project-name `--filter`. Cross-project prerequisites still run.
- Use `--concurrency <n>` to opt into bounded parallel execution. The default
  is one job. Parallel runs label recipe output, reserve inherited streams for
  exclusive interactive tasks, and check declared outputs for conflicts.
- Parallel failures stop new launches while active work finishes. Ctrl-C
  cleans up managed recipe processes on macOS, Linux, and Windows.

## [0.1.5]

- Tasks can opt into local caching with declared inputs, outputs, and
  environment variables. A cache hit verifies and restores the declared files.
- Use `lazel run --no-cache <project>:<task>` to run the recipes with the
  inherited environment and bypass the cache.
- The Rust/Python example exercises cache hits and output restoration. Native
  CLI tests cover invalidation, recovery, and concurrent runs.
- Moved updater unit tests to `tests/unit/update.rs`, named the CLI test
  `tests/update_cli.rs`, and kept the release eligibility test as an ignored
  local script.

## [0.1.4]

- Added `lazel update` and its alias `lazel upgrade` for the official per-user
  installations on macOS, Linux, and Windows, on x86-64 and ARM64.
- Self-update checks the latest stable GitHub release, verifies the archive's
  SHA-256 checksum and the executable's version, and skips equal or older
  releases. Updates are serialized and Windows retains the previous executable
  for recovery.
- Added offline updater tests and runs of the Rust test suite on all six release
  targets. Linux release builds install the musl C toolchain for the HTTPS client.
- Documented upgrading from older versions, which installations can self-update,
  and how to recover a previous Windows executable.
- Added this changelog and linked the tagged copy in GitHub release notes.
- Simplified the Windows PowerShell install command and recorded the Windows
  installer checks for 0.1.3.
- Added contributor guidance, a code of conduct, and a security reporting policy
  under `docs/`.
- Configured a Rust and Python development container for Codespaces and local
  development, with an optional firewall. Updated the OpenSpec integration to
  share project skills across agents.

## [0.1.3]

- Published native Windows x86-64 and ARM64 builds and added a PowerShell
  installer with SHA-256 verification.
- Fixed an unstable Rust API used for Windows project paths. Windows task recipes
  use Command Prompt and preserve the full exit code.
- Moved the Unix installer to `script/install.sh` and expanded the release
  workflow to six targets, with checks for both installers.

## 0.1.2

- The first Windows release tag failed CI before publication. The fix shipped
  in 0.1.3.

## [0.1.1]

- Published macOS and Linux binaries for x86-64 and ARM64, added SHA-256
  checksums, and introduced a Bash installer.

## 0.1.0

- Added workspace discovery and validation, dependency-ordered task execution,
  and shell exit status propagation on macOS and Linux.
- Added CLI integration tests, Rust CI, and a mixed Rust and Python example.

[0.1.5]: https://github.com/rohan-prasen/lazel/releases/tag/v0.1.5
[0.1.4]: https://github.com/rohan-prasen/lazel/releases/tag/v0.1.4
[0.1.3]: https://github.com/rohan-prasen/lazel/releases/tag/v0.1.3
[0.1.1]: https://github.com/rohan-prasen/lazel/releases/tag/v0.1.1
