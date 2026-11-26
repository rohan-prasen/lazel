#![cfg(windows)]

use std::fs;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
use windows_sys::Win32::System::Console::{
    CTRL_C_EVENT, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NEW_CONSOLE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

#[test]
fn runs_quoted_cmd_recipes_in_project_directory_and_stops_on_failure() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("app café");
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
dir = 'app café'
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

    for (jobs, expected) in [
        ("--concurrency=1", "inherited value"),
        ("--concurrency=2", "[app:check] inherited value"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_lazel"))
            .current_dir(&nested)
            .env("LAZEL_TEST_VALUE", "inherited value")
            .args(["run", "app:build", jobs])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(300), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("lazel: running app:prepare"), "{stderr}");
        assert!(
            stderr.contains("app:check' failed with exit code 300"),
            "{stderr}"
        );
        assert!(!project.join("ran.txt").exists());
    }
}

#[test]
fn ctrl_c_cancels_the_windows_job_and_leaves_work_unstarted() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("wait.ps1"),
        "$PID | Set-Content -Encoding ascii child.pid\nStart-Sleep -Seconds 60\n",
    )
    .unwrap();
    fs::write(
        root.path().join("lazel.toml"),
        r#"
[projects.app]
dir = '.'
[projects.app.tasks.hold]
command = 'powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File wait.ps1'
[projects.app.tasks.build]
command = 'type nul > unexpected'
depends_on = ['app:hold']
"#,
    )
    .unwrap();
    let diagnostics = root.path().join("diagnostics");
    let mut child = Running(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "isolated_console_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("LAZEL_TEST_CONSOLE_ROOT", root.path())
            .creation_flags(CREATE_NEW_CONSOLE)
            .stdout(fs::File::create(&diagnostics).unwrap())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    assert_eq!(
        child.finish(),
        Some(0),
        "{:?}",
        fs::read_to_string(diagnostics)
    );
}

#[test]
#[ignore = "run inside the isolated console fixture"]
fn isolated_console_fixture() {
    unsafe extern "system" fn ignore_ctrl_c(event: u32) -> i32 {
        i32::from(event == CTRL_C_EVENT)
    }

    let root = PathBuf::from(std::env::var_os("LAZEL_TEST_CONSOLE_ROOT").unwrap());
    assert_ne!(unsafe { SetConsoleCtrlHandler(Some(ignore_ctrl_c), 1) }, 0);
    let mut lazel = Running(
        Command::new(env!("CARGO_BIN_EXE_lazel"))
            .current_dir(&root)
            .args(["run", "build", "--concurrency=2"])
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    let pid = loop {
        if let Some(pid) = fs::read_to_string(root.join("child.pid"))
            .ok()
            .and_then(|value| value.trim().parse().ok())
        {
            break pid;
        }
        assert!(
            lazel.0.try_wait().unwrap().is_none(),
            "Lazel exited before the recipe started"
        );
        assert!(
            Instant::now() < deadline,
            "Windows recipe did not reach its barrier"
        );
        thread::sleep(Duration::from_millis(10));
    };
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    assert!(!handle.is_null(), "{}", std::io::Error::last_os_error());
    let descendant = unsafe { OwnedHandle::from_raw_handle(handle) };
    assert_ne!(unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0) }, 0);
    assert_eq!(lazel.finish(), Some(130));
    assert_eq!(
        unsafe { WaitForSingleObject(descendant.as_raw_handle(), 1000) },
        WAIT_OBJECT_0,
        "the recipe descendant survived cancellation"
    );
    assert!(!root.join("unexpected").exists());
}

struct Running(Child);

impl Running {
    fn finish(&mut self) -> Option<i32> {
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                return status.code();
            }
            assert!(
                Instant::now() < deadline,
                "Windows console fixture timed out"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
