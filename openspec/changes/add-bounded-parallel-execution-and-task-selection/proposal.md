## Why

Lazel can run one named target and its prerequisites, so checking several projects takes several invocations and independent tasks wait for each other. The second milestone in [PLAN.md](../../../PLAN.md#2-bounded-parallel-execution-and-task-selection) lets users select a set of tasks and opt into concurrent execution.

## What Changes

- Accept multiple bare task names and qualified `project:task` selectors in one `lazel run`. Add one optional `--filter <project-pattern>` for requested targets and include their full prerequisite closure.
- Add `--concurrency <n>` with a default of one. Preserve the existing serial behavior and let independent tasks overlap when users request two or more jobs. Count verified cache restores as completed prerequisites.
- Stop launching tasks after a failure and wait for active work to finish. On Ctrl-C during a parallel run, terminate managed task process trees and wait for cleanup on macOS, Linux, and Windows.
- Label parallel recipe output, give explicitly interactive tasks exclusive access to the inherited streams, and reject overlapping declared outputs for tasks that can run together.

## Capabilities

### New Capabilities

### Modified Capabilities

- `task-runner` covers multi-target selection, bounded scheduling, parallel stream and interruption behavior, and validation of declared output conflicts.

## Impact

- Update the CLI in `src/main.rs`, graph selection and validation in `src/workspace.rs`, and scheduling and process lifecycle in `src/commands/run.rs` and its platform support.
- Reuse the local cache's fingerprinting, workspace lock, and verified restore path. Extend output conflict checks to declarations on uncached tasks in a parallel plan.
- Extend CLI fixtures on Linux, macOS, Windows x86-64, and Windows ARM64. Document the flags, output ownership, and interactive task setting in the README and changelog.
