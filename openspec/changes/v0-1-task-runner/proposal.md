## Why

Teams with multiple languages in a monorepo need one command to run existing build tools in dependency order. Lazel v0.1 proves that workflow with a Python app that consumes output from a Rust generator.

## What Changes

- Discover a root `lazel.toml` from the current directory and validate its project paths, task definitions, and dependency graph.
- Execute `lazel run <project>:<task>` and its prerequisites through `/bin/sh`, preserving command output and exit status.
- Ship a Rust and Python example and verify it on macOS and Linux in CI.

## Capabilities

### New Capabilities

- `task-runner` covers workspace discovery, task selection, graph validation, and sequential shell execution.

### Modified Capabilities

## Impact

The Rust CLI gains TOML and Serde dependencies, integration tests, a documented example, and a Rust CI workflow. Caching and parallel execution are subsequent milestones.
