# Lazel design

Lazel coordinates existing build commands across languages. The first release proves this with a Python app that builds using a Rust generator's output.

The approved scope is one workspace config, shell command strings, dependency-ordered execution, and support for macOS and Linux. The [OpenSpec design](../../../openspec/changes/v0-1-task-runner/design.md) records the approaches and tradeoffs. The [task runner spec](../../../openspec/changes/v0-1-task-runner/specs/task-runner/spec.md) defines the CLI contract and failure behavior.

Local caching is the next milestone. It will be opt-in and will use declared inputs, outputs, and environment. A warm build must reuse valid results and restore missing artifacts. Changing an input or a prerequisite must invalidate affected results. Bounded parallel execution follows. Evaluate affected-task selection, remote caching, and build isolation after usage of the local workflow.
