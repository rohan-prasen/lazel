## Context

Lazel starts with a Clap dispatcher and a placeholder `run` handler. The first release must coordinate existing commands across language boundaries on macOS and Linux.

## Goals and boundaries

A single task identifier selects a reproducible execution order for a workspace build. Caching, concurrent jobs, native Windows, and build isolation have their own release decisions.

## Decisions

- Centralize definitions in `lazel.toml`. Per-project files would put definitions closer to their source code and require discovery and cross-file resolution. A single file makes the full graph visible for the first release.
- Use explicit `project:task` identifiers. Projects can share common task names and dependencies remain unambiguous.
- Use Serde and TOML for the schema. Build a validated workspace by resolving paths before executing any shell command. Walk upwards across Git boundaries so nested checkouts can participate in the workspace.
- Use an iterative depth-first traversal with visiting and visited states. Append tasks after their dependencies to preserve declaration order and deduplicate shared prerequisites. Check the entire graph before selecting the requested execution order.
- Invoke POSIX shell command strings through `/bin/sh`. This preserves shell syntax teams already use and establishes a common shell contract across the two supported operating systems.

## Risks and trade-offs

- A shared configuration file may grow contentious in a larger monorepo. Per-project files remain a future design decision.
- Workspace commands inherit the user's privileges, environment, and terminal. Document the trust model alongside the example.
- Sequential execution gives a predictable order and leaves concurrency policy for a later release.
