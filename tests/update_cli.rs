use std::process::Command;

#[test]
fn both_update_names_are_visible_and_validate_the_install_path() {
    let fixture = tempfile::tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_lazel");
    let help = Command::new(binary).arg("--help").output().unwrap();
    assert!(help.status.success());
    let stdout = String::from_utf8(help.stdout).unwrap();
    assert!(stdout.contains("update"));
    assert!(stdout.contains("upgrade"));

    for alias in ["update", "upgrade"] {
        let output = Command::new(binary)
            .arg(alias)
            .current_dir(fixture.path())
            .env("HOME", fixture.path())
            .env("USERPROFILE", fixture.path())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("original installation method"), "{stderr}");
        assert!(stderr.contains("lazel"), "{stderr}");
        assert!(!fixture.path().join(".local").exists());
    }
}
