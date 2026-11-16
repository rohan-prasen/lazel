## Why

Windows users need to install and run Lazel on x86-64 and ARM64 without setting up a Unix shell. Both installers should show what they are doing and tell the user how to run Lazel after installation.

## What Changes

- Run Windows task recipes through `cmd.exe` in the configured project directory.
- Add native Windows x86-64 and ARM64 checks and release builds. Publish a ZIP and SHA-256 checksum for each Windows target.
- **BREAKING** Move `install.sh` from the repository root to `script/install.sh`. Add `script/install.ps1` for Windows and document both commands in the README.
- Report download, verification, and installation progress. Print a copyable PATH command when the install directory is missing from the current PATH.

## Capabilities

### New Capabilities

- `release-installation` covers prebuilt release assets and checksum-verified installation on macOS, Linux, and Windows.

### Modified Capabilities

- `task-runner` adds Windows shell execution, local-drive validation, and native exit status propagation.

## Impact

The change touches the Rust runner, Windows CLI tests, GitHub Actions, installers, and README. The task selector and TOML schema stay the same. Windows installation requires a published release containing the Windows assets.
