#!/usr/bin/env bash
set -euo pipefail

main() (
    platform="$(uname -s):$(uname -m)"
    case "$platform" in
        Darwin:arm64) target=aarch64-apple-darwin ;;
        Darwin:x86_64) target=x86_64-apple-darwin ;;
        Linux:aarch64|Linux:arm64) target=aarch64-unknown-linux-musl ;;
        Linux:x86_64) target=x86_64-unknown-linux-musl ;;
        *) printf 'Lazel has no prebuilt binary for %s.\n' "$platform" >&2; return 1 ;;
    esac

    if command -v sha256sum >/dev/null; then
        checksum=(sha256sum)
    elif command -v shasum >/dev/null; then
        checksum=(shasum -a 256)
    else
        printf 'Lazel requires sha256sum or shasum to verify the download.\n' >&2
        return 1
    fi

    install_dir="$HOME/.local/bin"
    destination="$install_dir/lazel"
    if [[ -d "$destination" ]]; then
        printf 'Cannot install Lazel because %s is a directory.\n' "$destination" >&2
        return 1
    fi
    mkdir -p "$install_dir"
    work_dir=$(mktemp -d "$install_dir/.lazel.XXXXXXXX")
    trap 'rm -rf -- "$work_dir"' EXIT

    archive="lazel-$target.tar.gz"
    url="https://github.com/rohan-prasen/lazel/releases/latest/download/$archive"
    curl --proto '=https' --proto-redir '=https' --retry 3 -fsSL "$url" -o "$work_dir/$archive"
    curl --proto '=https' --proto-redir '=https' --retry 3 -fsSL "$url.sha256" -o "$work_dir/$archive.sha256"

    read -r expected _ < "$work_dir/$archive.sha256"
    actual=$("${checksum[@]}" < "$work_dir/$archive")
    if [[ "${actual%% *}" != "$expected" ]]; then
        printf 'Lazel download failed SHA-256 verification.\n' >&2
        return 1
    fi

    tar -xOzf "$work_dir/$archive" lazel > "$work_dir/lazel"
    chmod 755 "$work_dir/lazel"
    mv -f -- "$work_dir/lazel" "$destination"
    printf 'Installed Lazel to %s\n' "$destination"
    if [[ ":$PATH:" != *":$install_dir:"* ]]; then
        printf 'Add Lazel to your PATH with: export PATH="$HOME/.local/bin:$PATH"\n'
    fi
)

main
