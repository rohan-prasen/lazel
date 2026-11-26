## ADDED Requirements

### Requirement: Bounded parallel scheduling

Lazel SHALL accept `--concurrency <n>` as a positive integer and default to one job. With more than one job, Lazel SHALL launch ready tasks up to the requested limit. Each task SHALL complete at most once. A task becomes ready when every prerequisite has completed successfully, including completion by verified cache restore. Lazel SHALL make a prerequisite result available for dependent fingerprinting only after a verified restore or successful cache publication.

#### Scenario: Diamond graph with two jobs

- **WHEN** `app` depends on `left` and `right`, both depend on `base`, and the limit is two
- **THEN** `base` completes once, `left` and `right` can overlap, and `app` starts after both branches finish successfully

#### Scenario: Cache hit unlocks dependent work

- **WHEN** a prerequisite has a valid cached result and its declared outputs are absent
- **THEN** Lazel restores and verifies those outputs before the dependent checks its inputs or starts its shell, and skips the prerequisite recipe

#### Scenario: Invalid concurrency

- **WHEN** the concurrency limit is zero, negative, malformed, or outside the supported integer range
- **THEN** Lazel reports a CLI error before running any task

### Requirement: Parallel streams and foreground tasks

With more than one job, Lazel SHALL label ordinary recipe output with the fully qualified task selector, retain stdout and stderr as distinct streams, and supply no shared input to those recipes. Lazel SHALL bound output buffering and preserve byte content within each task stream apart from framing added for labels. Tasks SHALL accept optional `interactive = true`, defaulting to false. Explicitly interactive tasks MUST have caching disabled. A ready interactive task SHALL have exclusive use of the inherited stdin, stdout, and stderr while it runs. Lazel SHALL identify it on stderr before handing over those streams.

#### Scenario: Independent task output

- **WHEN** two ordinary recipes write to both stdout and stderr during a parallel run
- **THEN** both streams carry task labels, their output remains distinguishable by task, and each ordinary recipe receives EOF on stdin

#### Scenario: Interactive task during a parallel run

- **WHEN** a ready interactive task comes up for launch while ordinary work is active
- **THEN** Lazel waits for active work, runs the interactive recipe alone with inherited streams, and resumes scheduling after it succeeds

### Requirement: Parallel interruption

On macOS, Linux, and Windows, Lazel SHALL stop launching work on Ctrl-C during a parallel run, terminate managed task process trees, wait for child and stream cleanup, restore terminal ownership if it changed, and return exit code `130`. Lazel SHALL let an in-progress cache replacement finish or roll back and SHALL publish no cache result for an interrupted recipe.

#### Scenario: Cancel tasks with descendants

- **WHEN** a parallel recipe starts a child process and the user interrupts the run with Ctrl-C
- **THEN** the managed tree is terminated, queued tasks remain unstarted, and Lazel returns `130` after cleanup

#### Scenario: Cancel a foreground task

- **WHEN** the user presses Ctrl-C with an interactive recipe in the foreground
- **THEN** Lazel cancels the run, restores control to the caller, and leaves queued tasks unstarted

### Requirement: Declared output conflicts in parallel plans

Lazel SHALL use project-relative `outputs` from both cacheable and uncached tasks for parallel preflight. For concurrency greater than one, Lazel SHALL reject selected tasks with equal or ancestor-related output roots if neither task depends transitively on the other and both can execute at the same time. Lazel SHALL compare roots in workspace coordinates and apply existing path safety rules. Workspace-wide cache output ownership rules still apply for every concurrency setting.

#### Scenario: Independent tasks with overlapping output roots

- **WHEN** two selected tasks that can run together declare the same root or nested roots, including across nested projects and with caching off
- **THEN** Lazel identifies both tasks and the conflicting root before any shell launches

#### Scenario: Dependencies order output writers

- **WHEN** uncached tasks with overlapping outputs are ordered by a direct or transitive prerequisite edge
- **THEN** Lazel allows the parallel plan and runs the writers in prerequisite order

## MODIFIED Requirements

### Requirement: Workspace validation and selection

Lazel SHALL accept one or more bare task names and qualified `project:task` selectors after `run`. Each bare name selects matching tasks across projects. An optional `--filter <project-pattern>` SHALL apply a case-sensitive project-name glob to the requested targets. Every operand MUST match at least one target after filtering, and Lazel SHALL deduplicate targets and include their transitive prerequisites regardless of the filter. The TOML configuration SHALL define `projects.<project>.dir` as a relative directory resolving inside the workspace and `projects.<project>.tasks.<task>.command` as a nonblank string. Project and task names SHALL match `[a-z][a-z0-9_-]*`. Task dependencies SHALL use fully qualified selectors in the optional `depends_on` list, which defaults to empty. Lazel SHALL reject unknown keys, invalid names or directories, unknown dependency targets, invalid filters, unknown or unmatched selectors, and cycles before running a command.

#### Scenario: Selection with prerequisites outside the filter

- **WHEN** the user runs `lazel run build lint --filter "app-*"` and an app build depends on `tools:generate`
- **THEN** Lazel selects the app builds and lints and includes `tools:generate` once

#### Scenario: Mixed or repeated selectors

- **WHEN** a bare name and one or more qualified selectors resolve to the same task
- **THEN** Lazel includes that task once and preserves the order of first selection

#### Scenario: Selector does not match a target

- **WHEN** an operand is unknown or has no matches after filtering, even when another operand matched
- **THEN** Lazel reports the unmatched operand and exits `1` before any task runs

#### Scenario: Invalid workspace

- **WHEN** any project directory escapes the workspace or any task has an unknown dependency or forms a cycle, including outside the selected plan
- **THEN** Lazel rejects the workspace with exit code `1` before launching any task

### Requirement: Ordered shell execution

Lazel SHALL complete each reachable task once, after its prerequisites, through a successful shell command or a verified cache hit. At concurrency one, Lazel SHALL visit requested operands in command-line order, visit bare-name matches in project-name order, visit prerequisites in declaration order, and inherit the caller's standard streams. Lazel SHALL use `/bin/sh -c` on macOS and Linux and `cmd.exe` on Windows in each task's project directory. The environment SHALL be inherited for uncached runs and for `--no-cache`. When `cache = true` and `--no-cache` is absent, a launched recipe SHALL use the cache environment contract even if its cache is bypassed. Windows project directories MUST resolve to a local drive path usable by `cmd.exe`. Lazel SHALL reject unsupported directories before starting commands and SHALL emit its own progress to stderr.

#### Scenario: Shared prerequisite

- **WHEN** `app` depends on `left` then `right`, both depend on `base`, and concurrency is one
- **THEN** Lazel completes `base`, `left`, `right`, and `app` sequentially in that order, with each task running or restoring once

#### Scenario: Windows recipe with quoted paths

- **WHEN** a user invokes an uncached Windows task from a nested directory and its recipe contains quoted paths with spaces
- **THEN** Lazel runs the recipe through `cmd.exe` from the configured project directory and preserves the recipe's quoting and inherited environment

#### Scenario: Windows network share project directory

- **WHEN** a Windows project directory resolves to a UNC path
- **THEN** Lazel reports the unsupported directory and exits `1` before launching any task

### Requirement: Task failure

Lazel SHALL identify failed tasks on stderr and return the first observed task failure's exit status. On macOS and Linux, Lazel SHALL return `128 + signal` for a shell terminated by a signal. On Windows, Lazel SHALL preserve the full native exit status. Configuration, selection, launch, and fatal cache errors SHALL return `1` when they are the cause of failure. After observing a failure, Lazel SHALL start no new tasks and SHALL let already active work finish and reap its children before exiting. Ctrl-C SHALL still cancel an active parallel run while it waits for that work.

#### Scenario: Failed prerequisite

- **WHEN** a serial prerequisite's shell exits with status `7`
- **THEN** Lazel exits `7` and every task later in the execution order remains unstarted

#### Scenario: Parallel failure while another task is active

- **WHEN** one recipe fails while an independent recipe is still running
- **THEN** Lazel starts neither queued tasks nor dependents, waits for the independent recipe to finish, and returns the first observed failure status

#### Scenario: Windows exit status above 255

- **WHEN** a Windows prerequisite exits with status `300` and is the first observed failure
- **THEN** Lazel exits `300`, identifies the failed prerequisite, and leaves queued work unstarted

### Requirement: Cache diagnostics and bypass

Lazel SHALL report cache hits, misses, and bypasses for opted-in tasks to stderr. `lazel run --no-cache` SHALL bypass result reads and writes for every selected task and prerequisite and run each recipe with the inherited environment and the stream policy for the chosen concurrency setting. A hit SHALL restore artifacts without replaying task output from an earlier invocation.

#### Scenario: Explicit cache bypass

- **WHEN** a user invokes `run --no-cache` with an otherwise valid cached result
- **THEN** all selected recipes run, opted-in tasks report bypass on stderr, and cached entries remain untouched
