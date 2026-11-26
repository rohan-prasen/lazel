use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    if cfg!(windows) {
        fs::write(
            root.path().join("wait.ps1"),
            r#"
param([string]$Branch, [int]$Code)
New-Item -ItemType File "$Branch.started" | Out-Null
$deadline = [DateTime]::UtcNow.AddSeconds(20)
while (!(Test-Path "$Branch.release")) {
    if ([DateTime]::UtcNow -ge $deadline) { exit 90 }
    Start-Sleep -Milliseconds 20
}
New-Item -ItemType File "$Branch.done" | Out-Null
exit $Code
"#,
        )
        .unwrap();
    }
    root
}

fn recipe(unix: &'static str, windows: &'static str) -> &'static str {
    if cfg!(windows) { windows } else { unix }
}

fn gated(branch: &str, code: i32) -> String {
    if cfg!(windows) {
        format!(
            "powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File wait.ps1 {branch} {code}"
        )
    } else {
        format!(
            "touch {branch}.started; tries=0; while [ \"$tries\" -lt 1000 ]; do if [ -f {branch}.release ]; then touch {branch}.done; exit {code}; fi; tries=$((tries + 1)); sleep 0.02; done; exit 90"
        )
    }
}

fn run(root: &Path, tasks: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lazel"));
    command
        .current_dir(root)
        .args(["run", "--concurrency=2"])
        .args(tasks);
    command
}

struct Running(Child);

impl Running {
    fn spawn(command: &mut Command) -> Self {
        Self(command.stdin(Stdio::null()).spawn().unwrap())
    }

    fn wait_for(&mut self, path: &Path) {
        self.wait_until(|| path.exists(), &format!("creation of {path:?}"));
    }

    fn wait_until(&mut self, check: impl Fn() -> bool, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !check() {
            assert!(
                self.0.try_wait().unwrap().is_none(),
                "run exited before {expected}"
            );
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {expected}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn finish(&mut self) -> i32 {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                return status.code().unwrap_or(1);
            }
            assert!(Instant::now() < deadline, "run did not finish");
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

#[test]
fn overlaps_two_diamond_branches_and_waits_for_a_free_slot() {
    let fixture = fixture();
    let root = fixture.path();
    let left = gated("left", 0);
    let right = gated("right", 0);
    let base = recipe("echo base >> base.count", ">>base.count echo base");
    let final_check = recipe(
        "test -f left.done && test -f right.done && touch build.done",
        "if not exist left.done exit /b 91 & if not exist right.done exit /b 91 & type nul > build.done",
    );
    let third = recipe(
        "test -f left.done && touch queued.done",
        "if not exist left.done exit /b 92 & type nul > queued.done",
    );
    fs::write(
        root.join("lazel.toml"),
        format!(
            "
[projects.app]
dir = '.'
[projects.app.tasks.base]
command = '''{base}'''
[projects.app.tasks.left]
command = '''{left}'''
depends_on = ['app:base']
[projects.app.tasks.right]
command = '''{right}'''
depends_on = ['app:base']
[projects.app.tasks.build]
command = '''{final_check}'''
depends_on = ['app:left', 'app:right']
[projects.app.tasks.queued]
command = '''{third}'''
depends_on = ['app:base']
"
        ),
    )
    .unwrap();

    let mut running = Running::spawn(&mut run(root, &["build", "queued"]));
    running.wait_for(&root.join("left.started"));
    running.wait_for(&root.join("right.started"));
    assert!(!root.join("queued.done").exists());
    assert!(!root.join("build.done").exists());
    fs::write(root.join("left.release"), "").unwrap();
    running.wait_for(&root.join("queued.done"));
    assert!(!root.join("build.done").exists());
    fs::write(root.join("right.release"), "").unwrap();
    assert_eq!(running.finish(), 0);
    assert!(root.join("build.done").exists());
    assert_eq!(
        fs::read_to_string(root.join("base.count")).unwrap().trim(),
        "base"
    );
}

#[test]
fn stops_queuing_after_failure_and_waits_for_the_active_sibling() {
    let fixture = fixture();
    let root = fixture.path();
    let status = if cfg!(windows) { 300 } else { 7 };
    let failing = gated("first", status);
    let second = gated("second", 31);
    let unexpected = recipe("touch unexpected", "type nul > unexpected");
    fs::write(
        root.join("lazel.toml"),
        format!(
            "
[projects.app]
dir = '.'
[projects.app.tasks.first]
command = '''{failing}'''
[projects.app.tasks.second]
command = '''{second}'''
[projects.app.tasks.build]
command = '''{unexpected}'''
depends_on = ['app:first', 'app:second']
[projects.app.tasks.queued]
command = '''{unexpected}'''
"
        ),
    )
    .unwrap();
    let stderr = root.join("diagnostics");
    let mut running =
        Running::spawn(run(root, &["build", "queued"]).stderr(fs::File::create(&stderr).unwrap()));
    running.wait_for(&root.join("first.started"));
    running.wait_for(&root.join("second.started"));
    fs::write(root.join("first.release"), "").unwrap();
    running.wait_until(
        || {
            fs::read_to_string(&stderr)
                .unwrap()
                .contains(&format!("failed with exit code {status}"))
        },
        "the first failure",
    );
    fs::write(root.join("second.release"), "").unwrap();
    assert_eq!(running.finish(), status);
    assert!(root.join("second.done").exists());
    assert!(!root.join("unexpected").exists());
}

#[test]
fn parallel_launch_error_stops_dependents() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("removed")).unwrap();
    let unexpected = recipe("touch unexpected", "type nul > unexpected");
    fs::write(
        root.path().join("lazel.toml"),
        format!(
            "
[projects.app]
dir = '.'
[projects.app.tasks.remove]
command = 'rmdir removed'
[projects.app.tasks.build]
command = '''{unexpected}'''
depends_on = ['removed:work']
[projects.removed]
dir = 'removed'
[projects.removed.tasks.work]
command = 'echo unexpected'
depends_on = ['app:remove']
"
        ),
    )
    .unwrap();
    let result = run(root.path(), &["build"]).output().unwrap();
    assert_eq!(result.status.code(), Some(1), "{result:?}");
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("could not launch task 'removed:work'"),
        "{result:?}"
    );
    assert!(!root.path().join("unexpected").exists());
}

#[test]
fn labels_parallel_stdout_and_stderr_and_supplies_eof() {
    let root = tempfile::tempdir().unwrap();
    let first = recipe(
        "if read line; then exit 90; fi; echo eof; printf error >&2",
        "set /p line= & if errorlevel 1 echo eof & echo error 1>&2",
    );
    fs::write(
        root.path().join("lazel.toml"),
        format!(
            "
[projects.app]
dir = '.'
[projects.app.tasks.first]
command = '''{first}'''
[projects.app.tasks.second]
command = '''echo second && echo diagnostic 1>&2'''
"
        ),
    )
    .unwrap();
    let result = run(root.path(), &["first", "second"]).output().unwrap();
    assert!(result.status.success(), "{result:?}");
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("[app:first] eof"), "{stdout}");
    assert!(stdout.contains("[app:second] second"), "{stdout}");
    assert!(!stdout.contains("diagnostic"), "{stdout}");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("[app:first] error"), "{stderr}");
    assert!(stderr.contains("[app:second] diagnostic"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn streams_long_records_and_partial_non_utf8_bytes() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("lazel.toml"),
        r#"
[projects.app]
dir = '.'
[projects.app.tasks.build]
command = '''awk 'BEGIN { for (i = 0; i < 20000; i++) printf "x"; printf "\n" }'; printf '\377last' '''
"#,
    )
    .unwrap();
    let result = run(root.path(), &["build"]).output().unwrap();
    assert!(result.status.success(), "{result:?}");
    let records = result
        .stdout
        .split_inclusive(|&byte| byte == b'\n')
        .map(|line| {
            line.strip_prefix(b"[app:build] ")
                .unwrap()
                .strip_suffix(b"\n")
                .unwrap()
        })
        .collect::<Vec<_>>();
    let (last, long) = records.split_last().unwrap();
    assert_eq!(*last, b"\xfflast");
    assert!(long.len() > 1);
    assert_eq!(long.concat(), vec![b'x'; 20000]);
}

#[test]
fn interactive_recipe_uses_inherited_input_and_runs_exclusively() {
    let fixture = fixture();
    let root = fixture.path();
    let ordinary = gated("ordinary", 0);
    let interactive = recipe(
        "test -f ordinary.done && touch interactive.started && read line && printf 'LAZEL_LINE=%s\n' \"$line\" > received",
        "if not exist ordinary.done exit /b 90 & type nul > interactive.started & set /p LAZEL_LINE= & set LAZEL_LINE > received",
    );
    let queued = recipe(
        "test -f received && touch queued.done",
        "if not exist received exit /b 91 & type nul > queued.done",
    );
    fs::write(
        root.join("lazel.toml"),
        format!(
            "
[projects.app]
dir = '.'
[projects.app.tasks.ordinary]
command = '''{ordinary}'''
[projects.app.tasks.interactive]
command = '''{interactive}'''
interactive = true
[projects.app.tasks.queued]
command = '''{queued}'''
"
        ),
    )
    .unwrap();
    let mut running = Running(
        run(root, &["ordinary", "interactive", "queued"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    running.wait_for(&root.join("ordinary.started"));
    assert!(!root.join("interactive.started").exists());
    fs::write(root.join("ordinary.release"), "").unwrap();
    running.wait_for(&root.join("interactive.started"));
    assert!(!root.join("queued.done").exists());
    let line = recipe("actual input\n", "actual input\r\n");
    running
        .0
        .stdin
        .take()
        .unwrap()
        .write_all(line.as_bytes())
        .unwrap();
    assert_eq!(running.finish(), 0);
    assert_eq!(
        fs::read_to_string(root.join("received")).unwrap().trim(),
        "LAZEL_LINE=actual input"
    );
    assert!(root.join("queued.done").exists());
}
