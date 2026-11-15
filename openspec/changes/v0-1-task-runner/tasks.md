## 1. Run one configured task

- [x] 1.1 Run CLI tests against the scaffold, then implement discovery, configuration validation, and shell execution. Re-run the tests to verify process context, diagnostics, and exit codes.

## 2. Run a dependency graph

- [x] 2.1 Add and run failing graph tests, then implement full-graph preflight and ordered execution. Verify cross-project edges, declaration order, deduplication, and fail-fast behavior.

## 3. Prove and document v0.1

- [x] 3.1 Document the CLI and add a Rust and Python example. Verify generated and compiled outputs from both invocation directories and confirm that a repeat run executes both tasks.
- [x] 3.2 Add macOS and Linux CI jobs for formatting, Clippy, locked tests, and the example. Verify the workflow matrix and example invocation locally.
- [ ] 3.3 Confirm the hosted Linux and macOS jobs pass before release.

## Verification

- Single-task tests failed against the placeholder, then passed after the runner was implemented. Dependency tests failed against the single-task runner, then passed after graph validation and ordered execution were added.
- On macOS arm64, formatting, Clippy with warnings denied, 13 CLI integration tests, the locked build, and the mixed-language example passed. The example verifier ran from the workspace root and from `app/` and checked both artifacts and the compiled app's output.
- Strict OpenSpec validation, workflow YAML parsing, and independent whole-change review passed locally. Hosted CI and Linux execution are pending.
