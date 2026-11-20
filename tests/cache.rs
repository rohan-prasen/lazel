use std::fs::{self, File};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

fn run(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lazel"));
    command.current_dir(dir).arg("run");
    command
}

fn recipe(unix: &'static str, windows: &'static str) -> &'static str {
    if cfg!(windows) { windows } else { unix }
}

fn diagnostics(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn successful(output: Output) -> String {
    assert!(output.status.success(), "{output:?}");
    diagnostics(&output)
}

fn fixture() -> TempDir {
    let root = tempfile::Builder::new()
        .prefix("Lazel café ")
        .tempdir()
        .unwrap();
    fs::write(root.path().join("input.txt"), "original").unwrap();
    root
}

fn configure(dir: &Path, command: &str, fields: &str) {
    fs::write(dir.join("lazel.toml"), format!("[projects.app]\ndir = '.'\n[projects.app.tasks.build]\ncommand = '''{command}'''\n{fields}\n")).unwrap();
}

#[test]
fn rejects_invalid_cache_configuration_before_running() {
    let root = fixture();
    let dir = root.path();
    for (fields, expected) in [
        ("cache = true", "inputs"),
        ("cache = true\ninputs = ['src/[']", "glob"),
        ("cache = true\ninputs = ['../outside']", "input"),
        (
            "cache = true\ninputs = ['input.txt']\nenv = ['BAD=NAME']",
            "environment",
        ),
        (
            "cache = true\ninputs = ['input.txt']\noutputs = ['.lazel/cache']",
            "reserved",
        ),
        (
            "cache = true\ninputs = ['input.txt']\noutputs = ['../output']",
            "output",
        ),
    ] {
        configure(dir, "echo ran > ran.txt", fields);
        let output = run(dir).arg("app:build").output().unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(diagnostics(&output).contains(expected), "{output:?}");
        assert!(!dir.join("ran.txt").exists());
    }
}

#[test]
fn rejects_overlapping_roots_across_nested_projects() {
    let root = fixture();
    let dir = root.path();
    fs::create_dir(dir.join("nested")).unwrap();
    let child_output = if cfg!(windows) {
        "DIST/file"
    } else {
        "dist/file"
    };
    configure(
        dir,
        "echo ran > ran.txt",
        &format!(
            "cache = true\ninputs = ['input.txt']\noutputs = ['nested/dist']\n[projects.child]\ndir = 'nested'\n[projects.child.tasks.build]\ncommand = 'echo child'\ncache = true\ninputs = ['input.txt']\noutputs = ['{child_output}']"
        ),
    );
    let output = run(dir).arg("app:build").output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(diagnostics(&output).contains("overlapping"), "{output:?}");
    assert!(!dir.join("ran.txt").exists());
}

#[test]
fn restores_outputs_and_invalidates_changed_inputs() {
    let root = fixture();
    let dir = root.path();
    let command = recipe(
        "mkdir -p dist && cp input.txt dist/result.txt && echo ran >> ran.log",
        "(if not exist dist mkdir dist) && copy /y input.txt dist\\result.txt >nul && echo ran>>ran.log",
    );
    configure(
        dir,
        command,
        "cache = true\ninputs = ['*.txt', 'Cargo.lock']\noutputs = ['dist']\nenv = ['LAZEL_TEST_CONFIG']",
    );
    let build = |value: &str| {
        successful(
            run(dir)
                .arg("app:build")
                .env("LAZEL_TEST_CONFIG", value)
                .output()
                .unwrap(),
        )
    };
    assert!(build("first-setting").contains("cache miss"));
    fs::write(dir.join("dist/stale.txt"), "stale").unwrap();
    assert!(build("first-setting").contains("cache hit"));
    assert!(!dir.join("dist/stale.txt").exists());
    fs::remove_dir_all(dir.join("dist")).unwrap();
    assert!(build("first-setting").contains("cache hit"));
    assert_eq!(
        fs::read_to_string(dir.join("dist/result.txt")).unwrap(),
        "original"
    );
    assert_eq!(
        fs::read_to_string(dir.join("ran.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );

    fs::write(dir.join("input.txt"), "changed").unwrap();
    assert!(build("first-setting").contains("cache miss"));
    let changed = build("second-setting");
    assert!(changed.contains("cache miss"));
    assert!(!changed.contains("second-setting"));
    fs::write(dir.join("Cargo.lock"), "added").unwrap();
    assert!(build("second-setting").contains("cache miss"));
    fs::remove_file(dir.join("Cargo.lock")).unwrap();
    assert!(build("second-setting").contains("cache hit"));
}

#[test]
fn bypass_supplies_inherited_environment_and_leaves_cached_result_intact() {
    let root = fixture();
    let dir = root.path();
    let command = recipe(
        "printf '%s\\n' \"${LAZEL_TEST_INHERITED-unset}\" > result.txt",
        "(if defined LAZEL_TEST_INHERITED (echo %LAZEL_TEST_INHERITED%) else (echo unset)) >result.txt",
    );
    configure(
        dir,
        command,
        "cache = true\ninputs = ['input.txt']\noutputs = ['result.txt']",
    );
    let build = |options: &[&str]| {
        successful(
            run(dir)
                .args(options)
                .arg("app:build")
                .env("LAZEL_TEST_INHERITED", "inherited")
                .output()
                .unwrap(),
        )
    };
    assert!(build(&[]).contains("cache miss"));
    assert_eq!(
        fs::read_to_string(dir.join("result.txt")).unwrap().trim(),
        "unset"
    );
    assert!(build(&["--no-cache"]).contains("cache bypass"));
    assert_eq!(
        fs::read_to_string(dir.join("result.txt")).unwrap().trim(),
        "inherited"
    );
    assert!(build(&[]).contains("cache hit"));
    assert_eq!(
        fs::read_to_string(dir.join("result.txt")).unwrap().trim(),
        "unset"
    );
}

#[test]
fn explicit_bypass_preserves_shell_streams_and_status() {
    let root = fixture();
    let command = recipe(
        "printf '%s\\n' \"$LAZEL_TEST_VALUE\"; exit 7",
        "echo %LAZEL_TEST_VALUE% & exit /b 300",
    );
    configure(root.path(), command, "cache = true\ninputs = ['input.txt']");
    let output = run(root.path())
        .args(["--no-cache", "app:build"])
        .env("LAZEL_TEST_VALUE", "test value")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(if cfg!(windows) { 300 } else { 7 }),
        "{output:?}"
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "test value");
}

#[test]
fn cacheable_recipe_receives_declared_variables_only() {
    let root = fixture();
    let command = recipe(
        "echo \"$LAZEL_TEST_DECLARED\"; echo \"${LAZEL_TEST_HIDDEN-unset}\"",
        "echo %LAZEL_TEST_DECLARED% & if not defined LAZEL_TEST_HIDDEN echo unset",
    );
    configure(
        root.path(),
        command,
        "cache = true\ninputs = ['input.txt']\nenv = ['LAZEL_TEST_DECLARED']",
    );
    let declared = if cfg!(windows) {
        "lazel_test_declared"
    } else {
        "LAZEL_TEST_DECLARED"
    };
    let output = run(root.path())
        .arg("app:build")
        .env(declared, "declared")
        .env("LAZEL_TEST_HIDDEN", "hidden")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .collect::<Vec<_>>(),
        ["declared", "unset"]
    );
}

#[test]
fn skips_empty_results_but_never_reuses_a_failure() {
    let root = fixture();
    let dir = root.path();
    let fields = "cache = true\ninputs = ['input.txt']";
    configure(dir, "echo ran >> ran.log", fields);
    assert!(successful(run(dir).arg("app:build").output().unwrap()).contains("cache miss"));
    assert!(successful(run(dir).arg("app:build").output().unwrap()).contains("cache hit"));
    configure(
        dir,
        recipe(
            "echo failed >> ran.log; exit 7",
            "echo failed>>ran.log & exit /b 7",
        ),
        fields,
    );
    for _ in 0..2 {
        let output = run(dir).arg("app:build").output().unwrap();
        assert_eq!(output.status.code(), Some(7), "{output:?}");
        assert!(diagnostics(&output).contains("cache miss"));
    }
    assert_eq!(
        fs::read_to_string(dir.join("ran.log"))
            .unwrap()
            .lines()
            .count(),
        3
    );
}

#[test]
fn coordinates_concurrent_invocations() {
    let root = fixture();
    let dir = root.path();
    let contents = recipe(
        "echo ready > ready.txt\nread line\necho build >> ran.log\n",
        "@echo off\r\necho ready>ready.txt\r\nset /p LAZEL_SIGNAL=\r\necho build>>ran.log\r\n",
    );
    fs::write(
        dir.join(if cfg!(windows) { "work.cmd" } else { "work.sh" }),
        contents,
    )
    .unwrap();
    configure(
        dir,
        recipe("/bin/sh work.sh", "work.cmd"),
        "cache = true\ninputs = ['input.txt']",
    );
    let mut first = run(dir)
        .arg("app:build")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while !dir.join("ready.txt").exists() {
        if first.try_wait().unwrap().is_some() || Instant::now() >= until {
            let _ = first.kill();
            panic!(
                "first invocation did not start its recipe: {:?}",
                first.wait_with_output().unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let lock = File::options()
        .read(true)
        .write(true)
        .open(dir.join(".lazel/cache.lock"))
        .unwrap();
    assert!(lock.try_lock().is_err());
    let second = run(dir)
        .arg("app:build")
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(first.stdin.take());
    assert!(successful(first.wait_with_output().unwrap()).contains("cache miss"));
    assert!(successful(second.wait_with_output().unwrap()).contains("cache hit"));
    assert_eq!(
        fs::read_to_string(dir.join("ran.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn input_changes_during_execution_and_missing_outputs_are_never_published() {
    let root = fixture();
    let dir = root.path();
    configure(
        dir,
        "echo changed >> input.txt",
        "cache = true\ninputs = ['input.txt']",
    );
    for _ in 0..2 {
        let stderr = successful(run(dir).arg("app:build").output().unwrap());
        assert!(
            stderr.contains("cache miss") && stderr.contains("inputs changed"),
            "{stderr}"
        );
    }
    configure(
        dir,
        "echo done",
        "cache = true\ninputs = ['input.txt']\noutputs = ['missing.txt']",
    );
    for _ in 0..2 {
        let stderr = successful(run(dir).arg("app:build").output().unwrap());
        assert!(
            stderr.contains("cache miss") && stderr.contains("missing declared output"),
            "{stderr}"
        );
    }
}

#[test]
fn corrupt_cached_bytes_trigger_a_rebuild() {
    let root = fixture();
    let dir = root.path();
    configure(
        dir,
        "echo correct > output.txt",
        "cache = true\ninputs = ['input.txt']\noutputs = ['output.txt']",
    );
    let build = || successful(run(dir).arg("app:build").output().unwrap());
    assert!(build().contains("cache miss"));
    let entry = fs::read_dir(dir.join(".lazel/cache"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(entry.join("payload/output.txt"), "wrong bytes").unwrap();
    let stderr = build();
    assert!(
        stderr.contains("cache miss") && stderr.contains("invalid cache entry"),
        "{stderr}"
    );
    assert_eq!(
        fs::read_to_string(dir.join("output.txt")).unwrap().trim(),
        "correct"
    );
    assert!(build().contains("cache hit"));
}

#[test]
fn dependencies_restore_before_downstream_hashing_and_untracked_results_bypass() {
    let root = fixture();
    let dir = root.path();
    let copy = recipe(
        "cp generated.txt result.txt",
        "copy /y generated.txt result.txt >nul",
    );
    let config = format!(
        "[projects.app]\ndir = '.'\n[projects.app.tasks.generate]\ncommand = 'echo generated > generated.txt'\ncache = true\ninputs = ['input.txt']\noutputs = ['generated.txt']\n[projects.app.tasks.build]\ncommand = '{copy}'\ncache = true\ninputs = ['generated.txt']\noutputs = ['result.txt']\ndepends_on = ['app:generate']\n"
    );
    fs::write(dir.join("lazel.toml"), &config).unwrap();
    let build = || successful(run(dir).arg("app:build").output().unwrap());
    assert!(build().contains("cache miss app:build"));
    fs::remove_file(dir.join("generated.txt")).unwrap();
    fs::remove_file(dir.join("result.txt")).unwrap();
    let stderr = build();
    assert!(
        stderr.contains("cache hit app:generate") && stderr.contains("cache hit app:build"),
        "{stderr}"
    );
    assert_eq!(
        fs::read_to_string(dir.join("result.txt")).unwrap().trim(),
        "generated"
    );
    fs::write(
        dir.join("lazel.toml"),
        config.replacen("cache = true", "cache = false", 1),
    )
    .unwrap();
    assert!(build().contains("cache bypass app:build"));
}

#[cfg(unix)]
#[test]
fn unsafe_output_ancestors_leave_external_files_untouched() {
    use std::os::unix::fs::symlink;

    let root = fixture();
    let dir = root.path();
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("result.txt"), "external").unwrap();
    configure(
        dir,
        "mkdir -p dist && echo cached > dist/result.txt",
        "cache = true\ninputs = ['input.txt']\noutputs = ['dist/result.txt']",
    );
    assert!(successful(run(dir).arg("app:build").output().unwrap()).contains("cache miss"));
    fs::remove_dir_all(dir.join("dist")).unwrap();
    symlink(external.path(), dir.join("dist")).unwrap();
    let output = run(dir).arg("app:build").output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        fs::read_to_string(external.path().join("result.txt")).unwrap(),
        "external"
    );
}
