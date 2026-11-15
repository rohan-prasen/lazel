"""Exercise the release check against a temporary Git history."""

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


CHECK = Path(__file__).with_name("check-release.py").resolve()


class ReleaseCheckTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.repo = Path(directory.name)
        self.git("init", "--quiet")
        self.git("config", "user.name", "Release test")
        self.git("config", "user.email", "release-test@example.com")
        (self.repo / "src").mkdir()
        (self.repo / "src/main.rs").write_text("fn main() {}\n")
        self.write_cargo_files("0.1.1")
        self.commit()
        self.git("tag", "v0.1.1")

    def git(self, *args):
        subprocess.run(["git", *args], cwd=self.repo, check=True)

    def commit(self):
        self.git("add", ".")
        self.git("-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "Fixture")

    def write_cargo_files(self, version, dependency="1.0.0", locked_version=None):
        (self.repo / "Cargo.toml").write_text(
            f'[package]\nname = "lazel"\nversion = "{version}"\n'
            f'[dependencies]\nanyhow = "{dependency}"\n'
        )
        (self.repo / "Cargo.lock").write_text(
            f'[[package]]\nname = "lazel"\nversion = "{locked_version or version}"\n'
            f'[[package]]\nname = "anyhow"\nversion = "{dependency}"\n'
        )

    def check_candidate(self, tag, code, message):
        result = subprocess.run(
            [sys.executable, str(CHECK), tag, "v0.1.1"],
            cwd=self.repo,
            capture_output=True,
            text=True,
        )
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, code, output)
        self.assertIn(message, output)

    def test_docs_and_version_bump_are_insufficient(self):
        (self.repo / "README.md").write_text("Documentation\n")
        self.write_cargo_files("0.1.2")
        self.commit()
        self.check_candidate("v0.1.2", 1, "No source or dependency changes")

    def test_same_and_older_versions_are_rejected(self):
        for tag in ("v0.1.1", "v0.0.9"):
            with self.subTest(tag=tag):
                self.check_candidate(tag, 1, "must be newer")

    def test_tag_must_match_manifest(self):
        self.check_candidate("v0.1.2", 1, "does not match Cargo.toml")

    def test_lockfile_must_match_manifest(self):
        self.write_cargo_files("0.1.2", locked_version="0.1.1")
        self.commit()
        self.check_candidate("v0.1.2", 1, "Cargo.lock package version does not match")

    def test_source_change_committed_before_version_bump_counts(self):
        (self.repo / "src/main.rs").write_text('fn main() { println!("changed"); }\n')
        self.commit()
        self.write_cargo_files("0.1.10")
        self.commit()
        self.check_candidate("v0.1.10", 0, "is eligible")

    def test_dependency_change_qualifies(self):
        self.write_cargo_files("0.1.2", dependency="1.1.0")
        self.commit()
        self.check_candidate("v0.1.2", 0, "is eligible")


if __name__ == "__main__":
    unittest.main()
