## ADDED Requirements

### Requirement: Opt-in cache configuration

Task definitions SHALL accept `cache`, `inputs`, `outputs`, and `env`. `cache` SHALL default to `false`. Tasks with `cache = true` SHALL declare a nonempty list of project-relative input glob patterns. `outputs` SHALL list project-relative file paths or dedicated directory roots and SHALL default to an empty list. `env` SHALL list the names of environment variables supplied by the caller and SHALL default to an empty list. The runner SHALL reject invalid declarations, paths that escape the project or target workspace metadata, and output roots that overlap within or across cacheable tasks during workspace preflight. A result with no file outputs SHALL be cacheable.

#### Scenario: Existing task configuration

- **WHEN** a workspace declares tasks without cache fields
- **THEN** Lazel runs their shell commands as before and creates no cache state for the invocation

#### Scenario: Conflicting output roots

- **WHEN** two cacheable tasks claim the same output root or one claims an ancestor of the other's output root
- **THEN** Lazel reports the conflict and exits `1` before starting a shell, even if only one claimant was selected

### Requirement: Cache identity and environment

Lazel SHALL fingerprint the effective command and arguments, task identity, workspace-relative working directory, declared input and output configuration, matched input paths and content, effective environment, platform and declared toolchain inputs, and prerequisite results. It SHALL use a versioned SHA-256 fingerprint and distinguish unset environment variables from empty values. For cacheable execution without `--no-cache`, Lazel SHALL supply only the declared environment variables and documented platform defaults. Lazel SHALL hash inputs after prerequisites run or restore. It SHALL bypass caching when a prerequisite has no reliable result fingerprint. Cache diagnostics SHALL keep environment values private.

#### Scenario: Input and prerequisite invalidation

- **WHEN** a source file, declared lockfile, effective environment value, or declared toolchain identifier changes between otherwise equivalent runs
- **THEN** Lazel misses the affected task's previous entry and invalidates dependent tasks through the prerequisite result fingerprint

#### Scenario: Generated inputs and uncached prerequisites

- **WHEN** a task's prerequisite completes by running or restoring declared outputs
- **THEN** Lazel checks the dependent task's inputs after the prerequisite completes and bypasses the dependent task's cache if that prerequisite has no reliable result fingerprint

### Requirement: Store and restore local results

Lazel SHALL store complete, successful task results with a versioned manifest under `.lazel/cache/` at the workspace root. It SHALL verify the result and all output boundaries before restoring declared outputs, preserve executable permissions where supported, and skip the recipe on a verified hit. Failed or interrupted recipes SHALL publish no result. Lazel SHALL coordinate concurrent cache-aware invocations in the same workspace, including invocations that bypass cache data access, so cache publication and output replacement cannot conflict.

#### Scenario: Restore deleted outputs

- **WHEN** an unchanged task has a matching valid entry and its declared output files or directories have been deleted
- **THEN** Lazel recreates the declared outputs, including executable artifacts, before any dependent task runs and does not invoke the cached recipe

#### Scenario: Corrupt or unsafe entry

- **WHEN** an entry is incomplete, its contents fail integrity verification, or an output would traverse a symlink, reparse point, or forbidden path
- **THEN** Lazel installs none of the invalid entry and treats it as a cache miss only when it can safely run the recipe

#### Scenario: Restore fails during replacement

- **WHEN** the runner cannot finish replacing declared outputs
- **THEN** it restores the previous outputs if possible and reports an error with the recovery location if rollback is incomplete, without starting dependent tasks

#### Scenario: Concurrent invocations

- **WHEN** two invocations in the same workspace attempt to produce or restore a cacheable result simultaneously
- **THEN** Lazel coordinates them so each invocation observes only complete cache entries and their output writes do not overlap in time

### Requirement: Cache diagnostics and bypass

Lazel SHALL report cache hits, misses, and bypasses for opted-in tasks to stderr. `lazel run --no-cache <project>:<task>` SHALL bypass result reads and writes for the selected tasks and their prerequisites and execute their recipes with the inherited environment and standard streams. A hit SHALL restore artifacts without replaying task output from an earlier invocation.

#### Scenario: Explicit cache bypass

- **WHEN** a user invokes `run --no-cache` with an otherwise valid cached result
- **THEN** all selected recipes run normally, the opted-in tasks report bypass on stderr, and the cached result is left untouched

## MODIFIED Requirements

### Requirement: Ordered shell execution

Lazel SHALL complete each reachable task once, after its prerequisites, visiting prerequisites in declaration order. A task completes through a successful shell command or a verified cache hit. Lazel SHALL use `/bin/sh -c` on macOS and Linux and `cmd.exe` on Windows. It SHALL run each command in its project directory with inherited standard streams. The environment SHALL be inherited for uncached runs and for `--no-cache`. When `cache = true` and `--no-cache` is absent, the command SHALL use the cache environment contract even if its cache was bypassed. Windows project directories MUST resolve to a local drive path usable by `cmd.exe`. Lazel SHALL reject unsupported project directories before starting commands and SHALL emit its own progress to stderr.

#### Scenario: Shared prerequisite

- **WHEN** `app` depends on `left` then `right`, and both depend on `base`
- **THEN** Lazel completes `base`, `left`, `right`, and `app` sequentially in that order, with each task either running or restoring once

#### Scenario: Windows recipe with quoted paths

- **WHEN** a user invokes an uncached Windows task from a nested directory and its recipe contains quoted paths with spaces
- **THEN** Lazel runs the recipe through `cmd.exe` from the configured project directory and preserves the recipe's quoting and inherited environment

#### Scenario: Windows network share project directory

- **WHEN** a Windows project directory resolves to a UNC path
- **THEN** Lazel reports the unsupported directory and exits `1` before launching any task
