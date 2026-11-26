## 1. Select and validate the task plan

- [x] 1.1 Extend `src/main.rs` and `src/workspace.rs` with multiple operands, project-name glob filtering, and a positive job limit defaulting to one. Cover mixed and repeated selectors, project order, out-of-filter prerequisites, malformed and unmatched operands, invalid limits, and invalid graph nodes outside the selection. Update the legacy fixture in `tests/run.rs` that treats a bare task name as invalid.
- [x] 1.2 Derive prerequisite counts and reverse edges from the selected DFS order. Verify the diamond plan, shared targets and repeated edges, and that the existing serial dependency order stays intact.
- [x] 1.3 Parse optional `interactive = true` and reject its use with caching enabled. Retain normalized output declarations for uncached tasks and preflight conflicts in parallel plans. Verify equal and nested roots across projects, a cacheable and uncached pair, transitive ordering, exclusive tasks, and the existing workspace-wide cache conflict check at both limits. Verify serial runs still accept overlapping uncached declarations.

## 2. Schedule tasks and finalize cache results

- [ ] 2.1 Separate task preparation, shell launch, and result finalization in `src/commands/run.rs`. Keep a shared launch description and existing cache fingerprint order, environment, quoting, streams, and exit statuses at one job. Verify `tests/run.rs`, `tests/run_windows.rs`, and `tests/cache.rs` still pass.
- [x] 2.2 Run both limits through the ready queue and enforce the configured slot count. Use handshake files or pipes with fixture deadlines for failure detection. Verify that a diamond's shared base runs once, both branches arrive at a barrier at concurrency two, an additional ready recipe waits for a slot, and the final recipe waits for both branches to finish.
- [x] 2.3 Hold the workspace cache lock through execution and serialize restore and store operations on the coordinator. Extend `tests/cache.rs` to verify independent restores, restored outputs before dependent fingerprinting, bypass propagation for missing fingerprints, `--no-cache`, and unchanged cache contents after failed recipes.
- [x] 2.4 Stop new launches after the first observed fatal error or nonzero exit and drain active recipes. Verify with synchronized fixtures that queued work and dependents stay unstarted, an active independent task finishes, and the first status is retained. Keep coverage for a fatal launch or restore error and for a Windows status above 255.

## 3. Manage streams, foreground access, and interruptions

- [x] 3.1 Give ordinary parallel tasks EOF on stdin and stream labeled stdout and stderr through bounded readers. Verify both destinations, concurrent writes, long records, byte content, and a partial final record. Keep the serial streams test as a compatibility check.
- [x] 3.2 Run ready interactive tasks exclusively with inherited streams. Verify that active work drains, a piped input reaches the foreground recipe, and queued work resumes only after it finishes. Cover a real terminal in the platform fixtures.
- [x] 3.3 Add Unix process-group ownership, foreground terminal handoff, interrupt relay, and cleanup. Use a pseudo-terminal fixture to deliver Ctrl-C and verify terminal restoration, exit `130`, and disappearance of managed children. Cover a descendant holding a stream open and an active recipe during failure drain.
- [ ] 3.4 Add Windows Job Object containment and console cancellation. Use an isolated console fixture to verify Ctrl-C, exit `130`, queued work suppression, and cleanup of descendants even when they hold stream handles. Run the existing quoting and work-directory assertions through the managed launch path.
- [x] 3.5 Exercise cancellation of a parallel run waiting for the workspace cache lock and interruption during a cache-aware run. Verify canceled recipes publish no result and an in-flight replacement settles without leaving partially restored outputs.

## 4. Document and verify the milestone

- [x] 4.1 Update `README.md` and `CHANGELOG.md` with CLI examples, the one-job default, filter scope, failure and interrupt handling, labeled streams, interactive configuration, and dependency or serial ordering for shared undeclared files.
- [ ] 4.2 Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, and `cargo build --locked`. Run the Rust/Python verifier and hosted Rust CI on Linux, macOS, Windows x86-64, and Windows ARM64. Confirm the process and terminal fixtures run on their native targets.
- [x] 4.3 Compare one- and two-job cold, warm, and restore runs on a representative workspace with independent branches. Record tasks completed, cache hits, and elapsed time. Use the synchronized fixtures as the correctness check for overlap and ordering.
