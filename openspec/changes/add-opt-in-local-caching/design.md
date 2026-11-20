## Context

[`Workspace::load` and `execution_order`](../../../src/workspace.rs) already validate the workspace and produce a prerequisite-first plan. [`commands::run::execute`](../../../src/commands/run.rs) consumes it and builds the platform shell command in one place. Workspace discovery produces a canonical path. On Windows the runner separately converts project paths to the local-drive form expected by `cmd.exe`. The [proposal](proposal.md) and [task-runner spec](specs/task-runner/spec.md) define the behavior this design adds.

## Goals / Non-Goals

**Goals**

- Keep the current CLI dispatch, graph traversal, shell quoting, streams, and exit handling as the execution path.
- Make cache identity deterministic, and make the declared output roots the boundary for every restore.

**Non-Goals**

- Remote storage, task scheduling changes, log capture and replay, cache eviction, and automatic toolchain discovery are later work.
- Cacheable recipes still follow a declared-input contract. Lazel's local cache does not sandbox the recipe's reads or writes.

## Decisions

### Add cache metadata to the existing task

Parse optional `cache`, `inputs`, `outputs`, and `env` in the existing strict TOML schema. Use the workspace's canonical root for filesystem checks and keep normalized workspace-relative project paths for hashing. Keep `Task.dir` in the shell-compatible form for process launch. Use project-relative globs for inputs and literal paths for output files or owned directory trees. Dedicated roots let Lazel replace generated trees as a unit and remove stale generated files. Globs for outputs would make ownership and rollback less predictable.

Validate pattern syntax, path containment, and conflicts before execution. Reserve `.lazel`, Git metadata, and workspace configuration from output declarations. Compare output roots in workspace coordinates across all tasks and account for filesystem aliases, including Windows case and alternate path forms. At runtime, walk relevant path components without following symlinks or Windows reparse points. Prune cache metadata, Git metadata, and the task's own output roots when expanding input globs. Include other generated input files when their declared prerequisite has completed. Use a vetted glob matcher for pattern semantics and control traversal in Lazel.

### Fingerprint the exact launch environment and completed prerequisites

For `cache = true`, build one environment snapshot with `PATH`, Windows `SystemRoot` and `PATHEXT` where set, and variables named in `env`. Use that snapshot for both the key and `Command::env_clear` followed by explicit environment assignment. Normalize names on Windows, distinguish absent and empty values, and hash native values without lossy string conversion. Have recipes declare toolchain version identifiers and lockfiles through `env` and `inputs`. This keeps project-specific version probes out of Lazel. With `--no-cache`, use the existing inherited environment for the entire selected plan.

Build a versioned SHA-256 key from length-delimited fields, the task selector, relative work directory, effective command and arguments, output declarations, the launch environment, OS and architecture, sorted input names and file digests, and prerequisite fingerprints in declaration order. Include an explicit schema version for cache key changes. Keep the canonical checkout root out of the key itself. Derive the prerequisite result fingerprint from its task key and verified output manifest, and make it available only after a verified hit or a complete successful store. An unavailable result fingerprint causes dependent cache bypass. The serial loop already gives Lazel the point to hash inputs after prerequisites finish. Keep one shell-launch path for cold runs, bypasses, and existing uncached runs.

### Publish complete entries and restore with rollback

Keep unpacked artifacts plus a versioned manifest under `.lazel/cache/`, keyed by the fingerprint. Record expected roots, entry paths, sizes and SHA-256 digests, directory entries, and executable permissions on Unix. Reuse `sha2`, Serde/TOML, and `tempfile`. Stage the complete entry beside the destination and publish by rename after a successful recipe and output verification. Leave the task uncached if a required output is missing or its input fingerprint changed during execution. A failed cache write can leave a successful task's output usable, but it supplies no prerequisite fingerprint.

On a hit, validate the manifest against the current task and verify every stored entry before changing outputs. Stage verified copies, including permissions, and validate all destination ancestors. Back up existing declared roots, then install the staged roots. Keep backups until every replacement succeeds. Roll back on failure and stop before launching dependents if replacement fails. Retain recovery data and print its location if rollback fails. Mark when replacement begins so a later cache-aware run can detect an interrupted replacement and report recovery before proceeding. Replace owned output roots only. A workspace or artifact path that fails a safety check must never be followed during cleanup or restoration.

Take one exclusive workspace lock for any selected plan containing a cache-enabled task, including `--no-cache` runs. Hold it through the last task. Reuse the standard-library file-locking approach used in the updater. A per-key lock would leave simultaneous invocations with different keys free to overwrite the same workspace outputs. Existing invocations with zero selected cache-enabled tasks need no cache lock.

## Risks / Trade-offs

- Undeclared toolchain changes, network inputs, or time-dependent recipes can produce stale results. Document how to pin or declare these inputs and use uncached execution for nonreproducible work.
- Replacing an owned output directory also clears stale contents. Document that output roots belong exclusively to the recipe, reject conflicts during preflight, and leave other workspace files alone.
- One workspace lock trades concurrent throughput for safety while running and restoring outputs. Cacheable runs in separate workspaces stay independent.
- Windows sharing restrictions and filesystem boundaries can make rename fail. Verify all artifacts first, preserve the prior outputs until commit, and exercise rollback and diagnostics with targeted fixtures.

## Migration Plan

Existing config files continue to run with caching off. Document the opt-in fields, environment defaults, cache status messages, and `--no-cache` in the README. Update the mixed-language example to illustrate declarations and pin toolchain identity in its verifier. Users can remove cache fields to return to the prior behavior and delete the workspace's `.lazel/cache/` directory when Lazel is idle to free generated cache data.
