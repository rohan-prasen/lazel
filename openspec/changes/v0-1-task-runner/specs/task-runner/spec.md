## Purpose

Run existing build commands across projects and languages from one workspace configuration, with predictable dependency order and failure handling.

## ADDED Requirements

### Requirement: Workspace discovery

Lazel SHALL use the nearest `lazel.toml` while walking from the working directory to the filesystem root regardless of Git boundaries. It SHALL distinguish absent, unreadable, and invalid configuration.

#### Scenario: Invocation from a nested project

- **WHEN** the working directory is under a workspace containing `lazel.toml`
- **THEN** Lazel uses the nearest such file even when a Git boundary intervenes

#### Scenario: Invalid nearest config

- **WHEN** the nearest configuration is invalid and an ancestor also has a valid configuration
- **THEN** Lazel reports the invalid configuration and exits `1` before running any commands

### Requirement: Workspace validation and selection

Lazel SHALL accept `run <project>:<task>`. The TOML configuration SHALL define `projects.<project>.dir` as a relative directory resolving inside the workspace and `projects.<project>.tasks.<task>.command` as a nonblank string. Project and task names SHALL match `[a-z][a-z0-9_-]*`. Task dependencies SHALL use fully qualified selectors in the optional `depends_on` list, which defaults to empty. Lazel SHALL reject unknown keys, invalid names or directories, unknown dependency targets, unknown selectors, and cycles before running a command.

#### Scenario: Invalid workspace

- **WHEN** any project directory escapes the workspace or any task has an unknown dependency or forms a cycle, including outside the selected task's dependencies
- **THEN** Lazel rejects the workspace with exit code `1` before launching any task

### Requirement: Ordered shell execution

Lazel SHALL run each reachable task once, after its prerequisites, visiting prerequisites in declaration order. It SHALL use `/bin/sh -c` in each task's project directory with inherited environment and standard streams. Lazel SHALL emit its progress to stderr and support execution on macOS and Linux.

#### Scenario: Shared prerequisite

- **WHEN** `app` depends on `left` then `right`, and both depend on `base`
- **THEN** Lazel runs `base`, `left`, `right`, and `app` sequentially in that order

### Requirement: Task failure

Lazel SHALL stop at the first shell failure, identify the failed task on stderr, and return the shell's nonzero exit code or `128 + signal` if the shell was terminated by a signal. Configuration, selection, and launch errors SHALL return `1`.

#### Scenario: Failed prerequisite

- **WHEN** a prerequisite's shell exits with status `7`
- **THEN** Lazel exits `7` and every task later in the execution order remains unstarted
