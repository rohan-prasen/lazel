## 1. Native Windows task execution

- [x] 1.1 Add `cmd.exe` execution, local-drive path validation, and full Windows exit status propagation. Add a Windows CLI test for quoted paths, inherited environment, dependency order, and exit status `300`.

## 2. Release builds and installers

- [x] 2.1 Add both Windows targets to the CI and release matrices. Verify the YAML maps x86-64 and ARM64 to the respective native runners and packages ZIPs with adjacent checksums.
- [x] 2.2 Move the Bash installer and add the PowerShell installer. Check the `script/` layout and add fixture checks for clean installation, upgrade, PATH guidance, and checksum failure.
- [x] 2.3 Update the README and repository guide. Verify both installer links point to the scripts in `script/` and the Windows example uses Command Prompt syntax.

## 3. End-to-end verification

- [x] 3.1 On macOS, run formatting, Clippy, the locked build, the CLI and release eligibility tests, the mixed-language example, and the Bash installer fixture against a built archive.
- [x] 3.2 Confirm the release tag passes native Windows CLI and release package checks on x86-64 and ARM64, including the installer fixture under Windows PowerShell 5.1 and PowerShell 7.
- [ ] 3.3 Run the live PowerShell install command and `lazel --version` on both architectures using the published release.

## Verification

- The macOS run passed all 13 CLI tests and all six release eligibility tests. The mixed-language example passed from the example root and the nested app directory. The Bash fixture verified clean install, upgrade, and preservation of the installed binary on checksum failure.
- The `v0.1.3` release workflow passed native Windows CLI and both PowerShell installer fixture checks on x86-64 and ARM64. Both published Windows ZIPs matched their checksums and their executables matched the corresponding native PE architectures. [Release workflow](https://github.com/rohan-prasen/lazel/actions/runs/43807283695) · [Published assets](https://github.com/rohan-prasen/lazel/releases/tag/v0.1.3)
