"""Check a candidate release at HEAD before starting Rust jobs."""

import re
import subprocess
import sys
import tomllib

BUILD_TABLES = (
    "dependencies",
    "dev-dependencies",
    "build-dependencies",
    "target",
    "features",
    "patch",
    "replace",
)


def version_tuple(tag):
    if not re.fullmatch(r"v(0|[1-9][0-9]*)(\.(0|[1-9][0-9]*)){2}", tag):
        raise ValueError(f"Expected a vX.Y.Z release tag, got {tag!r}")
    return tuple(map(int, tag[1:].split(".")))


def git(*args):
    return subprocess.check_output(["git", *args], text=True)


def cargo_inputs(ref, tag):
    manifest = tomllib.loads(git("show", f"{ref}:Cargo.toml"))
    package = manifest["package"]
    if package["version"] != tag[1:]:
        raise ValueError(f"Tag {tag} does not match Cargo.toml version {package['version']}")

    lockfile = tomllib.loads(git("show", f"{ref}:Cargo.lock"))
    for locked in lockfile["package"]:
        if locked["name"] == package["name"] and "source" not in locked:
            if locked.pop("version") != package["version"]:
                raise ValueError(f"Cargo.lock package version does not match {tag}")
            break
    else:
        raise ValueError(f"Cargo.lock has no entry for {package['name']}")

    build_config = {key: manifest[key] for key in BUILD_TABLES if key in manifest}
    return build_config, lockfile


def check_release(tag, previous_tag):
    if version_tuple(tag) <= version_tuple(previous_tag):
        raise ValueError(f"Tag {tag} must be newer than the latest release {previous_tag}")

    previous = f"refs/tags/{previous_tag}"
    changed_source = git("diff", "--name-only", previous, "HEAD", "--", "src/", "build.rs")
    current_inputs = cargo_inputs("HEAD", tag)
    previous_inputs = cargo_inputs(previous, previous_tag)
    if not changed_source and current_inputs == previous_inputs:
        raise ValueError(f"No source or dependency changes since {previous_tag}")


if __name__ == "__main__":
    try:
        check_release(*sys.argv[1:])
    except (ValueError, KeyError, subprocess.CalledProcessError) as error:
        sys.exit(f"Release ineligible: {error}")
    print(f"Release {sys.argv[1]} is eligible.")
