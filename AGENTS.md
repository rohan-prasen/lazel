# Repository Guide

## Project status

Lazel is a Rust CLI for a language-agnostic monorepo task orchestrator.
The repository currently contains a CLI scaffold: `lazel run <task>` prints the
selected task and a message that execution is not implemented. Do not describe
task execution, workspace discovery, configuration, caching, or scheduling as
working features until they exist.

## Layout

- `Cargo.toml`: Rust 2024 binary package; `clap` with derive handles argument
  parsing and `anyhow` handles fallible command results.
- `src/main.rs`: CLI arguments, subcommands, and dispatch.
- `src/commands/mod.rs`: command module exports.
- `src/commands/run.rs`: `execute(&str)` handler for the `run` subcommand.
- `tests/`: integration test location; currently contains only `.gitkeep`.
- `README.md`: brief project description.
- `Cargo.lock`: committed dependency lockfile.
- `target/`: generated build output, ignored by Git.

## Development commands

Run commands from the repository root with a Rust toolchain supporting edition
2024. There is no pinned toolchain or CI workflow in this checkout.

```sh
cargo build --locked
cargo run --locked -- --help
cargo run --locked -- run build
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

The `run build` example selects a task named `build`; it does not build a
workspace. The test command currently discovers zero tests, so a successful exit
does not establish behavioral coverage. Use `cargo fmt` to apply formatting.

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
