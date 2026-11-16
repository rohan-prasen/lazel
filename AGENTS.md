# Repository Guide

## Agent workflow

- If the Ponytail plugin is installed in your agent configuration, always
  activate and follow it at the start of every task in this repository. Use
  `ponytail:ponytail` when the host exposes the skill under that name.
- Read [MEMORY.md](MEMORY.md) for the project history, release verification,
  and decisions that shape the current roadmap.
- After each tool call or edit that changes the project, append one dated,
  high-level bullet to the `Change log` in root `MEMORY.md` before continuing.
  Summarize what changed in one sentence. An editing tool call and its log
  entry count as one operation.

## Project status

Lazel is a Rust CLI for a language-agnostic monorepo task orchestrator.
`lazel run <project>:<task>` finds a `lazel.toml`, validates its configuration
and task graph, and runs the requested task and its prerequisites sequentially.
The installers distribute prebuilt binaries for macOS, Linux, and Windows on
x86-64 and ARM64. Opt-in local caching is the next milestone, followed by bounded
parallel execution.

## Layout

- `Cargo.toml`: Rust 2024 binary package; `clap` with derive handles argument
  parsing and `anyhow` handles fallible command results.
- `src/main.rs`: CLI arguments, subcommands, and dispatch.
- `src/commands/mod.rs`: command module exports.
- `src/commands/run.rs`: shell execution and task exit status.
- `src/workspace.rs`: workspace discovery, config validation, and execution order.
- `tests/`: CLI integration tests.
- `examples/mixed-language/`: Rust and Python build example and verification.
- `script/install.sh` and `script/install.ps1` verify and install the latest
  release for macOS/Linux and Windows, respectively.
- `.github/workflows/rust-ci.yml` checks builds, tests, and the example.
- `.github/workflows/release.yml` builds and publishes binaries on version tags.
- `README.md` documents installation, workspace configuration, and example usage.
- `Cargo.lock`: committed dependency lockfile.
- `target/`: generated build output, ignored by Git.

## Development commands

Run commands from the repository root with stable Rust. The example also
requires Python 3.

```sh
cargo build --locked
cargo run --locked -- --help
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
sh examples/mixed-language/verify.sh "$PWD/target/debug/lazel"
```

Use `cargo fmt` to apply formatting.

## Releases

Run routine checks locally. When source or dependency changes are ready to
ship, update the version in `Cargo.toml` and `Cargo.lock`, commit and push those
changes, then push the matching `vX.Y.Z` tag. The release workflow compares the
tagged commit with the latest published release before starting Rust jobs.
An eligible release runs the macOS, Linux, and Windows checks, builds the six
release targets, and publishes the binaries and checksums on GitHub.
Run `python3 .github/scripts/test_check_release.py` when changing the release
eligibility check.

## Change conventions

- Keep argument parsing and dispatch in `main.rs`, with command behavior in
  `src/commands/`. Wire new commands through both the module exports and CLI.
- Follow existing Rust formatting and naming. Reuse `clap` and `anyhow` rather
  than adding parallel parsing or error-handling mechanisms.
- Propagate failures with `Result` and `?`; avoid panics for user input or
  operational errors. Validate input before filesystem or process side effects.
- Keep changes scoped to the requested behavior. Check callers before changing
  a handler's signature or CLI behavior, and update affected documentation.
- Add focused unit tests beside new logic or CLI integration tests in `tests/`
  when implementing behavior. Keep placeholder output distinct from real work.
- Prefer verbose diagnostics during development; keep diagnostic output separate
  from normal command output and avoid logging secrets.
- Update `Cargo.lock` when dependencies change. Do not edit generated build
  output or unrelated local agent state.
- When preparing a release, keep the package version, lockfile, and `vX.Y.Z`
  tag aligned. Keep installer download names in sync with the release matrix.
- Run formatting, Clippy, and relevant tests for code changes. Report which
  checks ran and any missing coverage; do not claim unrun checks passed.
