## Context

[`Workspace::execution_order`](../../../src/workspace.rs) walks the requested task first, then checks the rest of the workspace for invalid edges and cycles. The [runner](../../../src/commands/run.rs) executes the resulting order with a blocking shell call. It fingerprints each task after its prerequisites complete and holds one workspace cache lock for the run. [Local cache restoration](../../../src/cache/store.rs) uses the single `.lazel/restoring` recovery directory. The [task-runner delta](specs/task-runner/spec.md) defines the public behavior.

## Goals / Non-Goals

- Use one validated task plan, shell launch description, and cache completion path for both concurrency settings. Preserve the ordering and terminal behavior of existing serial callers.
- Scope execution to finite invocations. Watch mode, persistent services, project-graph expansion, affected selection, and run reports remain separate roadmap milestones.

## Decisions

### Select roots and build the plan from the existing traversal

In `src/main.rs`, parse a nonempty list of task operands, an optional project filter, and a positive integer concurrency limit. In `src/workspace.rs`, reuse the existing identifier validation and compile a single case-sensitive project-name glob with the existing `glob` dependency. Match project identifiers and support the usual `*`, `?`, and character classes. Resolve every operand before launching work. A qualified operand excluded by the filter is unmatched. Apply the filter only during root selection so cross-project prerequisites remain usable.

Run the existing iterative traversal over roots in operand order, using project-name order within a bare operand. Keep the selected closure in its current prerequisite-first order, then validate the remaining workspace using the same traversal. Give selected tasks plan indices and derive reverse edges and initial prerequisite counts from the selected closure. Count duplicate dependency edges consistently in both directions or deduplicate them for scheduling only. Hash prerequisite fingerprints in the original declaration order, including repeated entries, to keep cache identities stable. The DFS order preserves the current serial tie-break without a second graph traversal for execution.

### Keep one scheduler and serialize cache operations

Use a ready queue ordered by plan index. With a single slot the earliest remaining task is ready, so the scheduler reproduces the dependency-first traversal. With multiple slots, use the same order to choose among currently ready tasks. Drain observed completions and check for failure or cancellation before starting more work. A normal failure closes the launch queue and lets running recipes finish. Keep the first observed failure status and reap all active children. Handle cancellation during this drain as well.

Keep graph state, prerequisite fingerprints, and cache operations on the coordinator. Split the existing run path into task preparation, shell launch, and completion. Preparation checks boundaries and fingerprints only ready tasks. A verified restore completes work directly and releases its dependents. On shell success, retain the current input recheck and result publication rules before marking the task complete. A successful recipe without a verified result still releases dependents, which then bypass their cache. Successful active recipes can finish and publish after an unrelated failure. Interrupted recipes publish no result.

Retain the workspace cache lock for all selected plans that contain a cacheable task, including `--no-cache`, and hold it until child and cache work has settled. Serialize cache operations within the invocation so they can share the restore directory and its current rollback semantics. Arrange for a canceled parallel run to leave a contended lock wait and let any in-progress replacement finish or roll back before exit.

### Separate process management from shell construction

Share the current shell choice, Windows quoting, working directory, and effective environment across launch modes. Keep inherited streams and native status handling at concurrency one. For a noninteractive parallel slot, connect stdin to the null device and drain stdout and stderr concurrently. Prefix records with the task selector and serialize the labeled writes per destination. Bound partial-line buffering, split long records into labeled fragments, and preserve bytes through EOF. When an interactive task reaches the front of the ready queue, let existing work drain and give it exclusive use of inherited streams. Its launch notice identifies which task has the terminal.

On Unix, place managed tasks in their own process groups. When a real controlling terminal is available to Lazel, use a guarded foreground handoff for the exclusive recipe. Set up an interrupt relay in the recipe's foreground group and confirm readiness before letting it read stdin, since terminal Ctrl-C goes to that group. For redirected streams, keep the inherited descriptors and skip the terminal handoff. On cancellation, signal the managed groups, escalate to forced termination after a bounded grace period, and restore the caller's terminal group on every exit path. Keep group ownership through cleanup so a reaped or reused PID cannot be targeted. [Apple terminal process-group reference](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/tcsetpgrp.3.html)

On Windows, use a kill-on-close Job Object for each managed recipe. Attach the process at creation via the job-list startup attribute so it cannot create descendants before containment. Keep the platform boundary small and verify that `cmd.exe` quoting, environment, working directory, and console or redirected handles match the shared launch description. Use a console control handler to notify the coordinator. Fail the managed launch if containment cannot be established. [Windows job objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects) · [Process creation attributes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute) · [Console control handler](https://learn.microsoft.com/en-us/windows/console/setconsolectrlhandler)

Keep a slot occupied through process cleanup and output drain. Retire any remaining owned descendants after the recipe exits. Make Unix pipe readers cancellable so a descendant that escaped the managed group cannot hold a slot open indefinitely. Signal and console callbacks only notify the coordinator. Cleanup, cache writes, and exit status selection run outside callbacks.

### Check output roots before launch

Retain normalized output paths for tasks with caching off, where the current loader validates the declarations and discards them. Share their normalization and workspace path keys with cacheable tasks. Keep the existing workspace-wide rejection for overlapping cache owners, even in serial runs and with `--no-cache`.

For concurrency above one, validate the selected uncached output paths with the existing workspace boundary and link checks. Compare selected roots in canonical workspace-relative coordinates. For a collision, use the plan edges to determine whether one task depends on the other. Reject equal and ancestor-related roots for unordered pairs that can overlap in execution. An exclusive interactive task cannot overlap another task. Checking dependency reachability only for colliding paths keeps graph work proportionate to the conflicts.

## Risks / Trade-offs

- **Undeclared writes and generated inputs** → Recipes still own their filesystem behavior. Document using explicit dependency edges for shared writes or generated inputs and using one job when there is no safe order encoded in the graph.
- **Cache coordination** → Serialized filesystem operations can delay a ready launch while cache state is being verified or replaced. Preserve the current recoverable restore transaction and measure cold, warm, and restored runs.
- **Terminal and process behavior** → Exercise terminal-delivered Ctrl-C, Windows console control events, descendants that keep a pipe open, quoting, and PID or handle cleanup on native CI. Unix recipes that deliberately create a new session or escape their managed group fall outside this cleanup guarantee.
- **OpenSpec history** → The current `task-runner` requirements live in the pending baseline, Windows, and caching changes. Archive those in dependency order or reconcile their deltas before archiving this change so the modified requirements have a canonical target.

## Migration Plan

Keep one job as the default and retain existing workspace files and selectors. Document the new flags, labeled streams, and `interactive = true` for uncached recipes that need stdin during a parallel run. Keep cache keys and manifests compatible. Users can return a parallel invocation to inherited serial streams by setting `--concurrency 1` or omitting the flag.
