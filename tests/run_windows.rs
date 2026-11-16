#![cfg(windows)]

use std::fs;
use std::process::Command;

#[test]
fn runs_quoted_cmd_recipes_in_project_directory_and_stops_on_failure() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("app project");
    let nested = project.join("nested directory");
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        project.join("write output.cmd"),
        "@echo off\r\n>\"task output.txt\" echo %LAZEL_TEST_VALUE%\r\n",
    )
    .unwrap();
    fs::write(
        root.path().join("lazel.toml"),
        r#"
[projects.app]
dir = 'app project'
[projects.app.tasks.prepare]
command = '''"write output.cmd"'''
[projects.app.tasks.check]
command = '''type "task output.txt" && exit /b 300'''
depends_on = ['app:prepare']
[projects.app.tasks.build]
command = '''echo unexpected>ran.txt'''
depends_on = ['app:check']
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_lazel"))
        .current_dir(&nested)
        .env("LAZEL_TEST_VALUE", "inherited value")
        .args(["run", "app:build"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(300), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "inherited value"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("lazel: running app:prepare"), "{stderr}");
    assert!(
        stderr.contains("app:check' failed with exit code 300"),
        "{stderr}"
    );
    assert!(!project.join("ran.txt").exists());
}
