## 1. CLI and workspace preflight

- [x] 1.1 Extend the existing task config and workspace representation with cache declarations and the canonical workspace root. Verify that legacy configs still pass and that invalid input patterns, environment names, unsafe output roots, and overlapping declarations fail before spawning a task, including across nested project directories.
- [x] 1.2 Add `run --no-cache` in `src/main.rs` and reuse the shell launcher in `src/commands/run.rs`. Verify that ordinary and explicitly bypassed runs inherit their environment and streams and preserve native exit statuses and quoting.

## 2. Environment and fingerprints

- [x] 2.1 Build the cacheable task environment snapshot and use it for launch and hashing. Verify absent versus empty values, declared variables, exclusion of undeclared values, platform defaults, and case-insensitive names on Windows.
- [x] 2.2 Expand project-relative input patterns with safe traversal and compute the versioned fingerprint. Test deterministic ordering, normalized relative paths, file additions and deletions, changes in recipe, lockfile, toolchain, environment, and prerequisite result, and exclusion of cache metadata and the task's own output roots.

## 3. Storage and runner integration

- [x] 3.1 Add the workspace lock and stage and publish manifest-backed cache entries after successful commands. Verify empty output lists, missing declared outputs, changed inputs during execution, failed commands, and two synchronized invocations targeting the same workspace.
- [x] 3.2 Implement verification and staged restore with rollback and interrupted-restore detection. Verify corruption and unsafe paths are caught before installing artifacts, previous outputs survive failed replacement, and restoring a dedicated output root preserves Unix execution permissions and removes stale generated files.
- [x] 3.3 Wire lookup, restore, bypass, and publication into the existing serial task loop. Verify hits skip the recipe, dependencies observe restored outputs, unavailable prerequisite fingerprints bypass downstream caches, and `--no-cache` leaves existing entries untouched. Assert cache diagnostics go to stderr and contain no environment values.

## 4. Documentation and end-to-end verification

- [x] 4.1 Update README workspace configuration and opt the Rust/Python example into caching. Document output ownership, ignored cache files, environment defaults, toolchain declarations, and bypass. Verify the example starts cold, hits on an unchanged invocation, and restores runnable outputs after deletion. Exercise source, environment, and lockfile invalidation in a temporary workspace.
- [x] 4.2 Cover cached restoration and concurrent locking in a native Windows CLI fixture. Include local-drive paths with spaces or non-ASCII characters and a case-insensitive output conflict. Retain Unix CLI coverage for executable permissions and compatibility, and verify the existing CI matrix runs both fixture sets on their target platforms.
- [x] 4.3 Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, `cargo build --locked`, and `sh examples/mixed-language/verify.sh "$PWD/target/debug/lazel"` on a Unix host. Record native CI results for the platform-specific behavior before release.
