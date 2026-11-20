#!/usr/bin/env bash
set -euo pipefail

ask() {
    local question=$1 default=$2 answer
    while :; do
        printf '\n%s ' "$question" >&3
        IFS= read -r answer <&3 || return 1
        answer=${answer#"${answer%%[![:space:]]*}"}
        answer=${answer%"${answer##*[![:space:]]}"}
        case $answer in
            '') [[ $default == yes ]]; return ;;
            [Yy]|[Yy][Ee][Ss]) return 0 ;;
            [Nn]|[Nn][Oo]) return 1 ;;
            *) printf 'Please enter y or n.\n' >&3 ;;
        esac
    done
}

version_is_newer() {
    local IFS=. LC_ALL=C part
    local installed available
    read -r -a installed <<< "$1"
    read -r -a available <<< "$2"
    for part in 0 1 2; do
        if (( ${#installed[part]} != ${#available[part]} )); then
            (( ${#installed[part]} > ${#available[part]} ))
            return
        fi
        if [[ ${installed[part]} != "${available[part]}" ]]; then
            [[ ${installed[part]} > ${available[part]} ]]
            return
        fi
    done
    return 1
}

main() (
    interactive=no
    if [[ -t 1 && -z ${CI:-} && ${LAZEL_INSTALL_NONINTERACTIVE:-0} != 1 ]] &&
        { exec 3<>/dev/tty; } 2>/dev/null; then
        interactive=yes
    fi
    if [[ $interactive == yes && ${TERM:-dumb} != dumb && -z ${NO_COLOR:-} ]]; then
        printf '\n\033[1;36m  Laz\033[0m'
        sleep 0.08
        printf '\033[1;36mel installer\033[0m\n'
    else
        printf '\n  Lazel installer\n'
    fi
    printf '  Run build, test, and lint tasks in a mixed-language monorepo.\n\n'
    printf '[Check] Looking up the platform and latest release.\n'
    platform="$(uname -s):$(uname -m)"
    case "$platform" in
        Darwin:arm64) target=aarch64-apple-darwin ;;
        Darwin:x86_64) target=x86_64-apple-darwin ;;
        Linux:aarch64|Linux:arm64) target=aarch64-unknown-linux-musl ;;
        Linux:x86_64) target=x86_64-unknown-linux-musl ;;
        *) printf 'Lazel has no prebuilt binary for %s.\n' "$platform" >&2; return 1 ;;
    esac

    for tool in curl tar; do
        if ! command -v "$tool" >/dev/null; then
            printf 'Lazel requires %s to install.\n' "$tool" >&2
            return 1
        fi
    done
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
    if [[ -L $destination || ( -e $destination && ! -f $destination ) ]]; then
        printf 'Cannot install Lazel because %s is a link or is not a regular file.\n' "$destination" >&2
        return 1
    fi
    release_base=https://github.com/rohan-prasen/lazel/releases
    version_pattern='(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)'
    if ! release_url=$(curl --proto '=https' --proto-redir '=https' --head -LsSf \
        --connect-timeout 10 --max-time 30 --output /dev/null --write-out '%{url_effective}' "$release_base/latest"); then
        printf 'Could not find the latest Lazel release. Check your connection and %s/latest.\n' "$release_base" >&2
        return 1
    fi
    tag=${release_url#"$release_base/tag/"}
    if [[ ! $tag =~ ^v${version_pattern}$ ]]; then
        printf 'The latest Lazel release has an invalid release tag. See %s/latest.\n' "$release_base" >&2
        return 1
    fi
    printf 'Platform           %s\nDestination        %s\nAvailable release  %s\n' "$target" "$destination" "$tag"
    active=$(type -P lazel || true)
    if [[ -n $active && ! $active -ef $destination ]]; then
        printf 'First on PATH      %s\n' "$active"
    fi
    action=Install
    if [[ -e $destination ]]; then
        action=Reinstall
        if [[ -x $destination ]] && installed=$("$destination" --version 2>/dev/null) &&
            [[ $installed == 'lazel '* && ${installed#lazel } =~ ^${version_pattern}$ ]]; then
            installed=${installed#lazel }
            printf 'Installed version  v%s\n' "$installed"
            if version_is_newer "$installed" "${tag#v}"; then
                printf 'Keeping the newer installed version.\n'
                return 0
            fi
            if [[ $installed != "${tag#v}" ]]; then action=Update; fi
        else
            printf 'Installed version  unreadable (Reinstall replaces this copy)\n'
        fi
    fi
    printf 'Action             %s\n' "$action"
    if [[ $interactive == yes ]] && ! ask "$action Lazel? [Y/n]" yes; then
        printf '\nInstallation cancelled.\n'
        return 0
    fi

    printf '\n[Stage] Preparing the install directory.\n'
    mkdir -p "$install_dir"
    work_dir=$(mktemp -d "$install_dir/.lazel.XXXXXXXX")
    trap 'rm -rf -- "$work_dir"' EXIT

    archive="lazel-$target.tar.gz"
    printf '[Download] Fetching %s and its checksum.\n' "$archive"
    for asset in "$archive" "$archive.sha256"; do
        if ! curl --proto '=https' --proto-redir '=https' --retry 3 -LsSf \
            --connect-timeout 10 --max-time 120 \
            "$release_base/download/$tag/$asset" -o "$work_dir/$asset"; then
            printf 'Could not download %s. Check your connection and the assets at %s.\n' "$asset" "$release_url" >&2
            return 1
        fi
    done

    printf '[Verify] Checking the SHA-256 checksum.\n'
    if ! read -r expected _ < "$work_dir/$archive.sha256" || [[ ! "$expected" =~ ^[0-9a-f]{64}$ ]]; then
        printf 'The release has an invalid SHA-256 checksum file.\n' >&2
        return 1
    fi
    actual=$("${checksum[@]}" < "$work_dir/$archive")
    if [[ "${actual%% *}" != "$expected" ]]; then
        printf 'Lazel download failed SHA-256 verification.\n' >&2
        return 1
    fi

    printf '[Stage] Extracting and checking Lazel %s.\n' "$tag"
    if ! tar -xOzf "$work_dir/$archive" lazel > "$work_dir/lazel" || [[ ! -s "$work_dir/lazel" ]]; then
        printf 'Could not extract the Lazel executable from %s.\n' "$archive" >&2
        return 1
    fi
    chmod 755 "$work_dir/lazel"
    if ! reported=$("$work_dir/lazel" --version 2>/dev/null) || [[ $reported != "lazel ${tag#v}" ]]; then
        printf 'Expected lazel %s in the release archive. The staged executable reported a different or unreadable version.\n' "${tag#v}" >&2
        return 1
    fi
    if [[ -L $destination || ( -e $destination && ! -f $destination ) ]]; then
        printf 'Cannot replace %s because it changed to a link or is not a regular file.\n' "$destination" >&2
        return 1
    fi
    printf '[Install] Placing the verified executable.\n'
    if ! mv -f -- "$work_dir/lazel" "$destination"; then
        printf 'Could not replace %s. Check its permissions and try again.\n' "$destination" >&2
        return 1
    fi
    case $action in
        Install) outcome=Installed ;;
        Update) outcome=Updated ;;
        Reinstall) outcome=Reinstalled ;;
    esac
    printf '\n%s Lazel %s at %s\n' "$outcome" "$tag" "$destination"
    active=$(type -P lazel || true)
    if [[ -z $active || ! $active -ef $destination ]]; then
        printf 'To use Lazel in this shell, run:\n  export PATH="$HOME/.local/bin:$PATH"\n'
    fi
    printf 'Verify your installation with: lazel --version\n'
    if [[ $action == Install ]]; then
        printf 'Get started with: lazel --help\n'
        printf 'In a workspace with lazel.toml, try a task such as: lazel run app:build\n'
    else
        printf 'Release notes: %s\n' "$release_url"
    fi
    printf 'Update later with: lazel update\n'
    if [[ $interactive == yes ]] && ask 'Show Lazel help now? [y/N]' no; then
        printf '\n'
        "$destination" --help </dev/null || printf 'Could not show help. Try lazel --help in your shell.\n' >&2
    fi
)

main
