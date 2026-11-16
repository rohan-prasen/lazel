## Purpose

Distribute prebuilt Lazel executables and let users install them with checksum verification and clear feedback on macOS, Linux, and Windows.

## ADDED Requirements

### Requirement: Platform release assets

An eligible release SHALL publish macOS and Linux `.tar.gz` archives and Windows `.zip` archives for x86-64 and ARM64. Each archive SHALL contain the platform executable and both license files and have an adjacent `.sha256` asset.

#### Scenario: Native Windows builds

- **WHEN** an eligible version tag passes release checks
- **THEN** the release includes `lazel-x86_64-pc-windows-msvc.zip` and `lazel-aarch64-pc-windows-msvc.zip`, each with `lazel.exe` and a matching `.zip.sha256` asset

### Requirement: Verified per-user installation

`script/install.sh` SHALL install Lazel to `$HOME/.local/bin` on macOS and Linux. `script/install.ps1` SHALL install `lazel.exe` to `%USERPROFILE%\.local\bin` on Windows using PowerShell 5.1 or later. Each installer SHALL select the appropriate release asset, identify its target and progress, and verify the SHA-256 checksum before replacing any installed executable. The README SHALL document both commands, including `curl -LsSf` piped to `bash` for macOS and Linux.

#### Scenario: Windows ARM64 installation

- **WHEN** the PowerShell installer runs on 64-bit Windows on ARM with the release assets available
- **THEN** it downloads and verifies the ARM64 ZIP and installs `lazel.exe` in the user's install directory

#### Scenario: PATH guidance after installation

- **WHEN** an installation succeeds and its install directory is missing from PATH
- **THEN** the installer prints the installed location, a copyable PATH command, and `lazel --version` without changing the user's PATH settings

#### Scenario: Download, checksum, or extraction failure

- **WHEN** an installer cannot download its assets, the checksum fails verification, or extraction fails
- **THEN** it reports the failure and preserves the existing executable
