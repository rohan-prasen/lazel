# Lazel roadmap to Turborepo feature parity

Build a Turborepo-style monorepo runner that works across language ecosystems. A team should be able to run `build`, `lint`, or `test` across projects, share verified build outputs between developers and CI, run only affected work, and prepare a smaller workspace for deployment. A JavaScript team should be able to migrate a documented subset of its existing Turborepo configuration and compare the resulting task graphs.

## Starting point and compatibility

Lazel 0.1.4 already discovers the nearest `lazel.toml`, validates the full task graph, and runs `lazel run <project>:<task>` in prerequisite order. [`src/workspace.rs`](src/workspace.rs) owns discovery and validation. [`src/commands/run.rs`](src/commands/run.rs) runs the plan using the platform shell. Keep both as the foundation for new workflows. The [Rust/Python example](examples/mixed-language/lazel.toml) is the first end-to-end fixture.

Keep existing TOML files valid and preserve the exit status, environment, and standard streams for existing serial runs. Until users explicitly opt into parallel execution, preserve prerequisite declaration order and fail on the first error. Keep macOS, Linux, and Windows in the test matrix. Use `lazel.toml` as the native format and measure compatibility against a pinned Turborepo version and a published set of fixtures.

## 1. Opt-in local caching

**Ship first.** Add task-level `cache`, `inputs`, `outputs`, and `env` fields. Require explicit inputs for cacheable tasks. Start with project-relative input globs and output files or dedicated output directories. A task with an empty output list can still cache a successful check. Use the existing TOML loader and shell runner. [Turborepo caching](https://turborepo.dev/docs/crafting-your-repository/caching)

- Compute a versioned SHA-256 key from the task identity and relative working directory, effective recipe and arguments, declared input paths and contents, output declarations, declared environment values, platform and toolchain inputs, and prerequisite fingerprints. Distinguish unset from empty variables. Hash generated inputs after prerequisites finish or restore. Bypass caching for downstream work when a prerequisite has no reliable fingerprint.
- Give cacheable commands an explicit environment contract, including platform-required variables. Declare and hash lockfiles, toolchain identifiers, and any environment values that affect output. Keep cache keys independent of absolute checkout paths where recipes produce relocatable output. Treat tasks that rely on undeclared machine state as uncached.
- Store successful results under `.lazel/cache/` with a versioned manifest. Use the existing `sha2` and `tempfile` dependencies. Stage writes and publish complete entries atomically. Verify file integrity and output boundaries before restore, including symlinks and Windows reparse points. Protect conflicting writes and restore a complete result or report the failure. Treat corrupt entries as misses when the workspace is safe to retry.
- Show hit, miss, and bypass on stderr. Keep output capture and replay with the later run-reporting work. Add a cache bypass so developers can diagnose a recipe with its normal shell behavior.

**Acceptance.** Run the mixed-language build twice and prove that the second invocation skips both recipes. Delete the outputs and restore them from cache, including a runnable generator. Edit the generator, a declared environment variable, or a lockfile and verify the expected downstream misses. Add focused fixtures for failure, corruption, overlapping output declarations, and concurrent invocations. Exercise restoration on native Windows as well as Unix.

## 2. Bounded parallel execution and task selection

Add `lazel run build lint` to select matching tasks across projects, plus an explicit `--filter <project-pattern>` and `--concurrency <n>`. Turn the existing validated graph into a runnable plan with reverse edges and prerequisite counts. Reuse a single shell execution path for serial, parallel, and later watch runs. [Turborepo run reference](https://turborepo.dev/docs/reference/run)

- Schedule ready tasks up to the limit. A verified cache restore satisfies a prerequisite. Keep the default at one job initially. Define parallel failure behavior explicitly, including stopping new launches after failure and reaping active children. Handle interruption and process-tree cleanup on Unix and Windows.
- Keep serial stream behavior. In parallel mode, label output by task and restrict interactive work to one foreground task. Reject overlapping declared output roots for tasks that could run concurrently. Document that recipes writing shared undeclared files need an explicit dependency edge or serial execution.

**Acceptance.** A diamond graph shares its prerequisite once, independent tasks demonstrably overlap at concurrency two, and the dependent starts after both finish. Test using synchronization so assertions do not depend on a fixed timing threshold. Cover cache hits, failure propagation, output conflicts, and Ctrl-C cleanup.

## 3. Project dependency graph, affected runs, and inspection

Add explicit project dependencies for native workspaces. Use them to expand `^build` into builds of dependency projects. Retain cross-project `project:task` edges and traverse intermediary projects that lack the matching task. Keep the project graph separate from the task graph, then validate the expanded task graph with the existing cycle checks. [Turborepo package and task graphs](https://turborepo.dev/docs/core-concepts/package-and-task-graph)

- Add `--dry=json` and a graph export using the same plan as execution. Include selected tasks, dependencies, and selection reasons. Add a run summary with durations and cache outcomes, plus bounded stdout/stderr replay for cacheable noninteractive tasks. Keep secrets out of reports and give tasks with sensitive logs a way to disable capture.
- Add `--affected` with explicit Git base and head overrides. Map changed paths to project owners and declared shared inputs, expand to reverse dependents, then include all prerequisites normally needed by the selected tasks. Handle renames and deletions, root-level inputs, and missing history. Use conservative full selection when the change range cannot be resolved.

**Acceptance.** Changing a shared project selects its affected dependents while leaving an unrelated project idle. A shared root input selects all consumers. A dry run launches no commands and shows the same task set as execution. A failing or canceled run still produces an accurate summary.

## 4. Remote caching for CI and teams

Reuse the local cache key, artifact validation, and restore path. Make remote caching opt-in per workspace. Check local storage first, fetch a remote artifact on a miss, and upload a verified local result after a successful run. Support read-only credentials for untrusted CI jobs and reserve writes for trusted jobs. [Turborepo remote caching](https://turborepo.dev/docs/core-concepts/remote-caching)

- Reuse `ureq` for bounded HTTPS requests. Keep tokens and signing keys in the environment. Scope artifact keys and credentials to the intended repository or team. Verify artifact authenticity before extraction, impose size limits, and fall back to a local run on timeout, server error, or invalid content. Require explicit opt-in for uploading task logs.
- Target an existing service that implements the documented Turborepo Remote Cache HTTP protocol. Use Lazel's own artifact namespace and format. Claim service interoperability only after exercising it against a real implementation. [Remote Cache API](https://turborepo.dev/docs/openapi)

**Acceptance.** A trusted CI job populates the cache and a fresh checkout restores matching outputs without spawning the recipes. Exercise offline fallback, tampered artifacts, read-only mode, and concurrent uploads. Run the protocol fixtures in CI against a deterministic test endpoint.

## 5. Watch mode and persistent development tasks

Add `lazel watch <task>` using the project graph and scheduler. Debounce file events, map changes to inputs and dependents, and suppress events caused by cache restores and generated outputs. Reload the plan when workspace configuration changes. Add persistent tasks with optional restart on relevant changes and support running related services together. [Turborepo watch reference](https://turborepo.dev/docs/reference/watch)

**Acceptance.** With a frontend and a backend running, a shared source edit rebuilds the affected projects and restarts only the services configured to restart. Repeated output events settle without a rebuild loop. Closing the session cleans up both foreground and background process trees. Reject persistent tasks used as completion prerequisites.

## 6. Turborepo migration and JavaScript workspace discovery

Start with a pinned pnpm workspace fixture. Add `lazel import turbo` to emit a reviewable Lazel config from the workspace manifests and `turbo.json`. Read workspace declarations and lockfile metadata to resolve internal packages, scoped names, and scripts. Delegate script execution to the package manager. Map supported `dependsOn` forms, package overrides, inputs and outputs, global dependencies, environment, cache options, and persistent task settings. Report any setting that cannot be translated faithfully. [Turborepo repository structure](https://turborepo.dev/docs/crafting-your-repository/structuring-a-repository) · [Configuration reference](https://turborepo.dev/docs/reference/configuration)

**Acceptance.** On a pnpm fixture with a shared package, compare the normalized task nodes, dependency edges, selected tasks, and build outputs from `turbo run` and the imported Lazel configuration. Test scoped names, packages without scripts, root tasks, and unsupported settings. Publish the verified mapping and repeat the fixture approach for npm, Yarn, and Bun before claiming support for each.

## 7. Pruned workspaces for deployment

Add `lazel prune <project>` to emit a standalone partial workspace. Retain the project dependency closure, task prerequisites required for the target build, and declared root files. Generate an adjusted Lazel config and delegate manifest and lockfile pruning to the supported package-manager integration. Add a Docker layout that separates dependency manifests from sources so source edits can reuse the install layer. [Turborepo prune reference](https://turborepo.dev/docs/reference/prune)

**Acceptance.** Prune the JavaScript fixture and the Rust/Python example into fresh directories. Install with the pruned lockfile where applicable, build from the emitted workspace, and verify that unrelated projects are absent. Test that a source-only edit leaves the Docker dependency layer reusable.

## Delivery and verification

- Start with one change that implements and tests local caching end to end. Create an OpenSpec change for that milestone when implementation starts. Keep each later milestone independently releasable and document its public configuration and CLI behavior in the README and changelog.
- Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, and `cargo build --locked` for each implementation change. Extend the native CI matrix fixtures for platform-specific scheduling, filesystem, and process behavior. Keep the [mixed-language verification script](examples/mixed-language/verify.sh) in CI alongside the JavaScript migration fixture once it lands.
- Measure cold, warm, and restore runs and serial versus bounded parallel execution on representative graphs. Record task counts, cache hits, and wall time. For a parity release, demonstrate affected selection, a clean CI cache restore, watch cleanup, migration, and a pruned build on the supported fixtures. Publish a compatibility table for CLI flags, config translation, package managers, and remote cache services.
