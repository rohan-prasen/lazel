## MODIFIED Requirements

### Requirement: Ordered shell execution

Lazel SHALL run each reachable task once, after its prerequisites, visiting prerequisites in declaration order. It SHALL use `/bin/sh -c` on macOS and Linux and `cmd.exe` on Windows. It SHALL run each command in its project directory with inherited environment and standard streams. Windows project directories MUST resolve to a local drive path usable by `cmd.exe`. Lazel SHALL reject unsupported project directories before starting commands and SHALL emit its own progress to stderr.

#### Scenario: Shared prerequisite

- **WHEN** `app` depends on `left` then `right`, and both depend on `base`
- **THEN** Lazel runs `base`, `left`, `right`, and `app` sequentially in that order

#### Scenario: Windows recipe with quoted paths

- **WHEN** a user invokes a Windows task from a nested directory and its recipe contains quoted paths with spaces
- **THEN** Lazel runs the recipe through `cmd.exe` from the configured project directory and preserves the recipe's quoting and inherited environment

#### Scenario: Windows network share project directory

- **WHEN** a Windows project directory resolves to a UNC path
- **THEN** Lazel reports the unsupported directory and exits `1` before launching any task

### Requirement: Task failure

Lazel SHALL stop at the first shell failure, identify the failed task on stderr, and return the shell's nonzero exit code. On macOS and Linux, Lazel SHALL return `128 + signal` if the shell was terminated by a signal. On Windows, Lazel SHALL preserve the full native exit status. Configuration, selection, and launch errors SHALL return `1`.

#### Scenario: Failed prerequisite

- **WHEN** a prerequisite's shell exits with status `7`
- **THEN** Lazel exits `7` and every task later in the execution order remains unstarted

#### Scenario: Windows exit status above 255

- **WHEN** a Windows prerequisite exits with status `300`
- **THEN** Lazel exits `300`, identifies the failed prerequisite, and leaves every later task unstarted
