# Lazel v0.1 implementation plan

**Goal** — Run a mixed-language build and its prerequisites with `lazel run app:build`.

**Architecture** — Keep the Clap dispatcher. Load TOML into a validated workspace and plan execution with iterative depth-first traversal. Run the plan sequentially with `/bin/sh` and inherited streams.

**Spec** — [Approved Lazel design](../specs/2026-11-15-lazel-design.md) and [v0.1 task runner requirements](../../../openspec/changes/v0-1-task-runner/specs/task-runner/spec.md).

1. Add CLI tests for workspace discovery, config validation, and shell execution. Run them against the placeholder, then implement loading and execution in the workspace module and `run` handler. Re-run the tests.
2. Add failing CLI graph tests. Extend TOML with `depends_on`, preflight the entire dependency graph, and run the requested tasks in dependency order. Verify a diamond, a missing target, a cycle, and a failed prerequisite.
3. Document the CLI, add a fixture with a Rust generator and a Python app, and wire CI on macOS and Linux. Build from the example root and `app` directory and verify that both tasks run again and produce both outputs.

Track completion and verification in the [OpenSpec task list](../../../openspec/changes/v0-1-task-runner/tasks.md). The approved [roadmap](../specs/2026-11-15-lazel-design.md) defines the next milestones.
