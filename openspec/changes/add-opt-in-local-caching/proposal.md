## Why

Lazel runs every selected recipe on every invocation, even when its inputs and prerequisites produce the same result. Opt-in local caching lets developers skip repeat work and recover deleted build outputs while preserving the existing workflow for uncached tasks.

## What Changes

- Add `cache`, `inputs`, `outputs`, and `env` to task configuration. Cacheable tasks declare project-relative inputs and any owned output files or directories.
- Reuse successful task results from `.lazel/cache/` when the recipe, inputs, environment, toolchain, platform, and prerequisites match. Verify stored artifacts and restore outputs before starting dependent tasks.
- Keep task execution sequential and report cache hits, misses, and bypasses on stderr. Support `lazel run --no-cache <project>:<task>` for a run with the original inherited environment and streams.
- Extend the Rust/Python example and native CLI tests to cover reuse, output restoration, invalidation, recovery, and concurrent invocations.

## Capabilities

### New Capabilities

### Modified Capabilities

- `task-runner` gains opt-in cache configuration, fingerprinting, and artifact restoration. A reachable task can complete by restoring a verified result in its existing position in the execution order.

## Impact

- `src/main.rs`, `src/workspace.rs`, and `src/commands/run.rs` gain the CLI option and cache integration. A focused cache module owns fingerprinting and artifact storage. Use the existing `sha2` and `tempfile` dependencies for hashing and staging.
- `README.md`, the mixed-language example, and the Rust CI matrix document and verify the new behavior on macOS, Linux, and Windows.
