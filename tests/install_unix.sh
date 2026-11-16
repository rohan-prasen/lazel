#!/usr/bin/env bash
set -euo pipefail

archive="$(cd -- "$(dirname -- "$1")" && pwd)/$(basename -- "$1")"
installer="$(cd -- "$(dirname -- "$0")/.." && pwd)/script/install.sh"
fixture=$(mktemp -d)
trap 'rm -rf -- "$fixture"' EXIT

test_home="$fixture/home directory"
bin_dir="$test_home/.local/bin"
mkdir -p "$fixture/bin"
cat > "$fixture/bin/curl" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
url=${@: -3:1}
destination=${@: -1}
case "${url##*/}" in
    "${LAZEL_TEST_ARCHIVE##*/}") cp "$LAZEL_TEST_ARCHIVE" "$destination" ;;
    "${LAZEL_TEST_ARCHIVE##*/}.sha256")
        if [[ ${LAZEL_TEST_CORRUPT:-0} == 1 ]]; then
            printf '%064d\n' 0 > "$destination"
        else
            cp "$LAZEL_TEST_ARCHIVE.sha256" "$destination"
        fi ;;
    *) exit 22 ;;
esac
STUB
chmod +x "$fixture/bin/curl"

export LAZEL_TEST_ARCHIVE="$archive"
test_path="$fixture/bin:$PATH"
output=$(env HOME="$test_home" PATH="$test_path" bash < "$installer")
[[ "$output" == *'export PATH="$HOME/.local/bin:$PATH"'* ]]
[[ "$output" == *'lazel --version'* ]]
"$bin_dir/lazel" --version
cp "$bin_dir/lazel" "$fixture/installed"

printf 'old executable\n' > "$bin_dir/lazel"
output=$(env HOME="$test_home" PATH="$bin_dir:$test_path" bash < "$installer")
[[ "$output" != *'export PATH='* ]]
cmp "$fixture/installed" "$bin_dir/lazel"

if output=$(env HOME="$test_home" PATH="$test_path" LAZEL_TEST_CORRUPT=1 bash < "$installer" 2>&1); then
    printf 'The installer accepted a corrupt checksum.\n' >&2
    exit 1
fi
[[ "$output" == *'failed SHA-256 verification'* ]]
cmp "$fixture/installed" "$bin_dir/lazel"
[[ -z $(find "$bin_dir" -name '.lazel.*' -print) ]]
printf 'Unix installer checks passed.\n'
