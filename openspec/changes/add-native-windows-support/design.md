## Context

The [original task-runner spec](../v0-1-task-runner/specs/task-runner/spec.md) defines a sequential runner that uses `/bin/sh`. Releases already build four macOS and Linux binaries and gate publication on version, source, and dependency changes.

## Goals / Non-Goals

- Keep the task schema and dependency planner shared across platforms. Select the native shell at launch.
- Keep installation per user and unattended. Leave persistent PATH and shell profile updates to the user.
- Scope the Windows runner to x86-64 and ARM64 workspaces on local drives.

## Decisions

- Use `cmd.exe /D /V:OFF /S /C` with an explicitly quoted recipe. Both Windows PowerShell and Command Prompt are built in, and the approved Windows recipe syntax is Command Prompt. Git Bash would add a prerequisite. Convert validated verbatim drive paths to their ordinary drive path before launch and preserve the full Windows exit status.
- Build and execute checks on native x86-64 and ARM64 runners so both binaries exercise the actual Windows shell. Package `lazel.exe` and the licenses as ZIPs with adjacent `.sha256` files. Retain the existing Unix archives so earlier Unix release assets work with the relocated Bash installer.
- Use the OS processor architecture for the PowerShell download, including when PowerShell is running under emulation. Stage the archive and executable on the install volume. Verify the checksum before replacing the installed executable, then print the install location and PATH guidance.

## Risks and Trade-offs

- A network-share project directory would cause `cmd.exe` to fall back to a different working directory. Lazel rejects paths that Command Prompt cannot use before any task starts.
- The previous root `install.sh` URL moves. Use the README command that points to `script/install.sh`.
- Before the first release with Windows assets, the PowerShell script reports an unavailable download and points to the release page. A locked executable produces a replacement error with retry guidance.

## Release and Spec Order

- The existing version-tag workflow gates publication on CI and all six release builds. Verify both Windows ZIPs and checksums on the published release before testing the live PowerShell bootstrap.
- Promote the `v0-1-task-runner` baseline to `openspec/specs/task-runner/` before applying this change's task-runner delta.
