# Repository Guide

## Project status

Lazel is a Rust CLI for a language-agnostic monorepo task orchestrator.
`lazel run <project>:<task>` finds a `lazel.toml`, validates its configuration
and task graph, and runs the requested task and its prerequisites sequentially.
Local caching and parallel execution are future milestones.

## Layout

- `Cargo.toml`: Rust 2024 binary package; `clap` with derive handles argument
  parsing and `anyhow` handles fallible command results.
- `src/main.rs`: CLI arguments, subcommands, and dispatch.
- `src/commands/mod.rs`: command module exports.
- `src/commands/run.rs`: shell execution and task exit status.
- `src/workspace.rs`: workspace discovery, config validation, and execution order.
- `tests/`: CLI integration tests.
- `examples/mixed-language/`: Rust and Python build example and verification.
- `README.md`: workspace configuration and example usage.
- `Cargo.lock`: committed dependency lockfile.
- `target/`: generated build output, ignored by Git.

## Development commands

Run commands from the repository root with stable Rust. The example also
requires Python 3. GitHub Actions runs the checks on macOS and Linux.

```sh
cargo build --locked
cargo run --locked -- --help
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
sh examples/mixed-language/verify.sh "$PWD/target/debug/lazel"
```

Use `cargo fmt` to apply formatting.

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
- Run formatting, Clippy, and relevant tests for code changes. Report which
  checks ran and any missing coverage; do not claim unrun checks passed.
