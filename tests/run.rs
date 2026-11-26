#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn lazel(dir: &Path, task: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lazel"));
    command.current_dir(dir).args(["run", task]);
    command
}

fn configure(root: &Path, config: &str) {
    fs::write(root.join("lazel.toml"), config).unwrap();
}

fn single_task(command: &str) -> String {
    format!("[projects.app]\ndir = '.'\n[projects.app.tasks.build]\ncommand = '''{command}'''\n")
}

fn assert_error(output: Output, message: &str) {
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains(message), "{stderr:?}");
    assert!(output.stdout.is_empty(), "{:?}", output.stdout);
}

#[test]
fn runs_in_project_directory_with_inherited_environment_and_streams() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("app");
    let nested = project.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir(nested.join(".git")).unwrap();
    configure(
        root.path(),
        r#"
[projects.app]
dir = 'app'
[projects.app.tasks.build]
command = '''read -r input && pwd -P && printf "%s\n" "$LAZEL_TEST_VALUE:$input" && printf "recipe diagnostic\n" >&2'''
"#,
    );

    let mut child = lazel(&nested, "app:build")
        .env("LAZEL_TEST_VALUE", "inherited")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"stdin\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            "{}\ninherited:stdin\n",
            project.canonicalize().unwrap().display()
        )
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("lazel: running app:build"), "{stderr}");
    assert!(stderr.contains("recipe diagnostic"), "{stderr}");
}

#[test]
fn selects_nearest_workspace() {
    let root = tempfile::tempdir().unwrap();
    let inner = root.path().join("inner");
    fs::create_dir(&inner).unwrap();
    configure(root.path(), &single_task("printf outer"));
    configure(&inner, &single_task("printf inner"));

    let output = lazel(&inner, "app:build").output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"inner");
}

#[test]
fn reports_missing_workspace() {
    let root = tempfile::tempdir().unwrap();
    assert_error(
        lazel(root.path(), "app:build").output().unwrap(),
        "no lazel.toml found",
    );
}

#[test]
fn reports_invalid_nearer_config_without_falling_back() {
    let root = tempfile::tempdir().unwrap();
    let inner = root.path().join("inner");
    fs::create_dir(&inner).unwrap();
    configure(root.path(), &single_task("touch ran"));
    configure(&inner, "[invalid");

    assert_error(
        lazel(&inner, "app:build").output().unwrap(),
        "invalid workspace config",
    );
    assert!(!root.path().join("ran").exists());
}

#[test]
fn reports_unreadable_config() {
    let root = tempfile::tempdir().unwrap();
    symlink(root.path().join("missing"), root.path().join("lazel.toml")).unwrap();
    assert_error(
        lazel(root.path(), "app:build").output().unwrap(),
        "could not read workspace config",
    );
}

#[test]
fn rejects_escaping_project_before_executing_valid_task() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), root.path().join("outside")).unwrap();
    configure(
        root.path(),
        &format!(
            "{}\n[projects.other]\ndir = 'outside'\ntasks = {{}}\n",
            single_task("touch ran")
        ),
    );

    assert_error(
        lazel(root.path(), "app:build").output().unwrap(),
        "outside the workspace",
    );
    assert!(!root.path().join("ran").exists());
}

#[test]
fn rejects_invalid_configuration_before_execution() {
    let root = tempfile::tempdir().unwrap();
    let config = single_task("touch ran");
    for (invalid, message) in [
        (format!("{config}typo = []\n"), "unknown field"),
        (single_task("   "), "blank command"),
        (
            config.replace(".tasks.build]", ".tasks.BUILD]"),
            "invalid task name",
        ),
        (
            config.replace("dir = '.'", "dir = '/tmp'"),
            "must be relative",
        ),
        (
            config.replace("dir = '.'", "dir = 'missing'"),
            "project directory",
        ),
    ] {
        configure(root.path(), &invalid);
        assert_error(lazel(root.path(), "app:build").output().unwrap(), message);
        assert!(!root.path().join("ran").exists());
    }
}

#[test]
fn reports_invalid_and_unknown_selectors() {
    let root = tempfile::tempdir().unwrap();
    configure(root.path(), &single_task("touch ran"));
    assert_error(
        lazel(root.path(), "app::build").output().unwrap(),
        "expected task or project:task",
    );
    assert_error(
        lazel(root.path(), "app:missing").output().unwrap(),
        "unknown task 'app:missing'",
    );
    assert!(!root.path().join("ran").exists());
}

#[test]
fn preserves_shell_status_and_reports_failure() {
    let root = tempfile::tempdir().unwrap();
    for (command, code) in [("exit 7", 7), ("kill -TERM $$", 143)] {
        configure(root.path(), &single_task(command));
        let output = lazel(root.path(), "app:build").output().unwrap();
        assert_eq!(output.status.code(), Some(code), "{output:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains(&format!("app:build' failed with exit code {code}")),
            "{stderr}"
        );
    }
}

#[test]
fn runs_diamond_dependencies_once_in_declaration_order() {
    let root = tempfile::tempdir().unwrap();
    configure(
        root.path(),
        r#"
[projects.shared]
dir = '.'
[projects.shared.tasks.base]
command = 'echo base >> execution.log'
[projects.shared.tasks.left]
command = 'echo left >> execution.log'
depends_on = ['shared:base']
[projects.shared.tasks.right]
command = 'echo right >> execution.log'
depends_on = ['shared:base']
[projects.app]
dir = '.'
[projects.app.tasks.build]
command = 'echo app >> execution.log'
depends_on = ['shared:left', 'shared:right', 'shared:left']
[projects.app.tasks.unrelated]
command = 'echo unrelated >> execution.log'
"#,
    );

    let output = lazel(root.path(), "app:build").output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let log = root.path().join("execution.log");
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        "base\nleft\nright\napp\n"
    );

    let config = fs::read_to_string(root.path().join("lazel.toml")).unwrap();
    configure(
        root.path(),
        &config.replace(
            "['shared:left', 'shared:right', 'shared:left']",
            "['shared:right', 'shared:left', 'shared:right']",
        ),
    );
    fs::remove_file(&log).unwrap();
    let output = lazel(root.path(), "app:build").output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        "base\nright\nleft\napp\n"
    );
}

#[test]
fn validates_dependencies_of_unrelated_tasks_before_executing() {
    let root = tempfile::tempdir().unwrap();
    let app = single_task("touch ran");
    for (graph, message) in [
        (
            "[projects.app.tasks.other]\ncommand = 'true'\ndepends_on = ['missing:build']",
            "unknown dependency 'missing:build' for task 'app:other'",
        ),
        (
            "[projects.app.tasks.other]\ncommand = 'true'\ndepends_on = ['app:second']\n[projects.app.tasks.second]\ncommand = 'true'\ndepends_on = ['app:other']",
            "dependency cycle: app:other -> app:second -> app:other",
        ),
        (
            "[projects.app.tasks.other]\ncommand = 'true'\ndepends_on = ['app:other']",
            "dependency cycle: app:other -> app:other",
        ),
    ] {
        configure(root.path(), &format!("{app}\n{graph}\n"));
        assert_error(lazel(root.path(), "app:build").output().unwrap(), message);
        assert!(!root.path().join("ran").exists());
    }
}

#[test]
fn stops_before_later_tasks_when_a_prerequisite_fails() {
    let root = tempfile::tempdir().unwrap();
    configure(
        root.path(),
        r#"
[projects.app]
dir = '.'
[projects.app.tasks.build]
command = 'touch app-ran'
depends_on = ['app:first', 'app:second']
[projects.app.tasks.first]
command = 'exit 7'
[projects.app.tasks.second]
command = 'touch second-ran'
"#,
    );

    let output = lazel(root.path(), "app:build").output().unwrap();
    assert_eq!(output.status.code(), Some(7), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("app:first' failed with exit code 7"));
    assert!(!root.path().join("app-ran").exists());
    assert!(!root.path().join("second-ran").exists());
}

#[test]
fn reports_launch_failure_if_a_prerequisite_removes_a_project_directory() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("app")).unwrap();
    configure(
        root.path(),
        r#"
[projects.tools]
dir = '.'
[projects.tools.tasks.prepare]
command = 'rmdir app'
[projects.app]
dir = 'app'
[projects.app.tasks.build]
command = 'true'
depends_on = ['tools:prepare']
"#,
    );

    assert_error(
        lazel(root.path(), "app:build").output().unwrap(),
        "could not launch task 'app:build'",
    );
    assert!(!root.path().join("app").exists());
}
