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
    release_url="https://github.com/rohan-prasen/lazel/releases/latest"
    printf 'Downloading Lazel for %s\n' "$target"
    for asset in "$archive" "$archive.sha256"; do
        if ! curl --proto '=https' --proto-redir '=https' --retry 3 -LsSf \
            "$release_url/download/$asset" -o "$work_dir/$asset"; then
            printf 'Could not download %s. Check your connection and the assets at %s.\n' "$asset" "$release_url" >&2
            return 1
        fi
    done

    printf 'Verifying SHA-256 checksum\n'
    if ! read -r expected _ < "$work_dir/$archive.sha256" || [[ ! "$expected" =~ ^[0-9a-f]{64}$ ]]; then
        printf 'The release has an invalid SHA-256 checksum file.\n' >&2
        return 1
    fi
    actual=$("${checksum[@]}" < "$work_dir/$archive")
    if [[ "${actual%% *}" != "$expected" ]]; then
        printf 'Lazel download failed SHA-256 verification.\n' >&2
        return 1
    fi

    printf 'Installing Lazel to %s\n' "$install_dir"
    if ! tar -xOzf "$work_dir/$archive" lazel > "$work_dir/lazel" || [[ ! -s "$work_dir/lazel" ]]; then
        printf 'Could not extract the Lazel executable from %s.\n' "$archive" >&2
        return 1
    fi
    chmod 755 "$work_dir/lazel"
    if ! mv -f -- "$work_dir/lazel" "$destination"; then
        printf 'Could not replace %s. Check its permissions and try again.\n' "$destination" >&2
        return 1
    fi
    printf 'Installed Lazel to %s\n' "$destination"
    if [[ ":$PATH:" != *":$install_dir:"* ]]; then
        printf 'To use Lazel in this shell, run:\n  export PATH="$HOME/.local/bin:$PATH"\n'
    fi
    printf 'Verify your installation with: lazel --version\n'
)

main
