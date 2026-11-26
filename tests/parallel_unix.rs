#![cfg(unix)]

use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn wait_for<T>(mut check: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(value) = check() {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "fixture did not reach its barrier"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

struct Running(Child);

impl Running {
    fn finish(&mut self) -> i32 {
        wait_for(|| self.0.try_wait().unwrap()).code().unwrap()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if self.0.try_wait().is_ok_and(|status| status.is_none()) {
            unsafe { libc::kill(self.0.id() as libc::pid_t, libc::SIGINT) };
            let deadline = Instant::now() + Duration::from_secs(1);
            while Instant::now() < deadline {
                if self.0.try_wait().is_ok_and(|status| status.is_some()) {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn pid_file(path: &Path) -> libc::pid_t {
    wait_for(|| fs::read_to_string(path).ok()?.trim().parse().ok())
}

fn assert_stopped(pid: libc::pid_t) {
    wait_for(|| {
        let status = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "stat="])
            .output()
            .unwrap();
        let value = String::from_utf8_lossy(&status.stdout);
        // Descendants may briefly be zombies until their new parent reaps them.
        (!status.status.success() || value.trim_start().starts_with('Z')).then_some(())
    });
}

#[test]
fn ctrl_c_reaches_the_foreground_relay_and_returns_the_terminal() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("lazel.toml"),
        r#"
[projects.app]
dir = '.'
[projects.app.tasks.interactive]
interactive = true
command = '''read greeting; printf '%s' "$greeting" > received; trap '' INT TERM; sleep 60 & echo $! > child.pid; wait'''
[projects.app.tasks.queued]
command = 'touch unexpected'
"#,
    )
    .unwrap();
    let (master, slave) = terminal();
    let diagnostics = root.path().join("terminal.log");
    let mut caller = Command::new("/bin/sh");
    caller
        .current_dir(root.path())
        .args([
            "-c",
            r#""$1" run --concurrency=2 interactive queued; result=$?; touch caller.ready; read response; printf '%s' "$response" > caller.received; exit "$result""#,
            "terminal-caller",
            env!("CARGO_BIN_EXE_lazel"),
        ])
        .stdin(slave.try_clone().unwrap())
        .stdout(slave)
        .stderr(File::create(&diagnostics).unwrap());
    unsafe {
        caller.pre_exec(|| {
            if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut caller = Running(caller.spawn().unwrap());
    // Close the master before waiting on a failed caller so the PTY cannot block its exit.
    let mut master = master;
    let fd = master.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    assert_eq!(
        unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) },
        0
    );
    let mut drain = |master: &mut File| {
        let mut buffer = [0; 1024];
        while master.read(&mut buffer).is_ok_and(|n| n > 0) {}
        if let Some(status) = caller.0.try_wait().unwrap() {
            panic!(
                "terminal caller exited early with {status}: {:?}",
                fs::read_to_string(&diagnostics)
            );
        }
    };
    master.write_all(b"recipe input\n").unwrap();
    let pid = wait_for(|| {
        drain(&mut master);
        fs::read_to_string(root.path().join("child.pid"))
            .ok()?
            .trim()
            .parse()
            .ok()
    });
    assert_eq!(
        fs::read_to_string(root.path().join("received")).unwrap(),
        "recipe input"
    );
    master.write_all(b"\x03").unwrap();
    wait_for(|| {
        drain(&mut master);
        root.path().join("caller.ready").exists().then_some(())
    });
    master.write_all(b"caller input\n").unwrap();
    let exit = wait_for(|| {
        let mut buffer = [0; 1024];
        while master.read(&mut buffer).is_ok_and(|n| n > 0) {}
        caller.0.try_wait().unwrap()
    });
    assert_eq!(
        exit.code(),
        Some(130),
        "{:?}",
        fs::read_to_string(diagnostics)
    );
    assert_eq!(
        fs::read_to_string(root.path().join("caller.received")).unwrap(),
        "caller input"
    );
    assert!(!root.path().join("unexpected").exists());
    assert_stopped(pid);
}

fn terminal() -> (File, File) {
    unsafe {
        let mut master = -1;
        let mut slave = -1;
        assert_eq!(
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ),
            0,
            "{}",
            std::io::Error::last_os_error()
        );
        let master = File::from_raw_fd(master);
        let slave = File::from_raw_fd(slave);
        for fd in [master.as_raw_fd(), slave.as_raw_fd()] {
            assert_eq!(libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC), 0);
        }
        (master, slave)
    }
}

#[test]
fn cancels_children_during_the_failure_drain() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("lazel.toml"),
        r#"
[projects.app]
dir = '.'
[projects.app.tasks.first]
command = 'while [ ! -s child.pid ]; do sleep 0.02; done; exit 7'
[projects.app.tasks.active]
command = '''trap '' TERM; sleep 60 & echo $! > child.pid; wait'''
[projects.app.tasks.queued]
command = 'touch unexpected'
"#,
    )
    .unwrap();
    let diagnostics = root.path().join("diagnostics");
    let mut laz = Running(
        Command::new(env!("CARGO_BIN_EXE_lazel"))
            .current_dir(root.path())
            .args(["run", "--concurrency=2", "first", "active", "queued"])
            .stdin(Stdio::null())
            .stderr(File::create(&diagnostics).unwrap())
            .spawn()
            .unwrap(),
    );
    let pid = pid_file(&root.path().join("child.pid"));
    wait_for(|| {
        let stderr = fs::read_to_string(&diagnostics).unwrap();
        stderr.contains("failed with exit code 7").then_some(())
    });
    assert_eq!(
        unsafe { libc::kill(laz.0.id() as libc::pid_t, libc::SIGINT) },
        0
    );
    assert_eq!(laz.finish(), 130);
    assert!(!root.path().join("unexpected").exists());
    assert_stopped(pid);
}

#[test]
fn reaps_recipe_descendants_that_hold_output_pipes_open() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("lazel.toml"),
        r#"
[projects.app]
dir = '.'
[projects.app.tasks.build]
command = 'sleep 60 & echo $! > child.pid; printf done'
"#,
    )
    .unwrap();
    let stdout = root.path().join("stdout");
    let mut laz = Running(
        Command::new(env!("CARGO_BIN_EXE_lazel"))
            .current_dir(root.path())
            .args(["run", "--concurrency=2", "build"])
            .stdout(File::create(&stdout).unwrap())
            .spawn()
            .unwrap(),
    );
    assert_eq!(laz.finish(), 0);
    assert_eq!(fs::read_to_string(stdout).unwrap(), "[app:build] done\n");
    assert_stopped(pid_file(&root.path().join("child.pid")));
}

#[test]
fn cancels_a_waiting_cache_lock_and_never_publishes_an_interrupted_recipe() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path();
    fs::create_dir(dir.join(".lazel")).unwrap();
    let lock = File::create(dir.join(".lazel/cache.lock")).unwrap();
    lock.lock().unwrap();
    fs::write(dir.join("input"), "source").unwrap();
    fs::write(
        dir.join("lazel.toml"),
        r#"
[projects.app]
dir = '.'
[projects.app.tasks.build]
command = 'echo partial > result; sleep 60 & echo $! > child.pid; wait'
cache = true
inputs = ['input']
outputs = ['result']
"#,
    )
    .unwrap();
    let diagnostics = dir.join("diagnostics");
    let spawn = || {
        Running(
            Command::new(env!("CARGO_BIN_EXE_lazel"))
                .current_dir(dir)
                .args(["run", "--concurrency=2", "build"])
                .stderr(File::create(&diagnostics).unwrap())
                .spawn()
                .unwrap(),
        )
    };
    let mut waiting = spawn();
    wait_for(|| {
        let stderr = fs::read_to_string(&diagnostics).unwrap();
        stderr
            .contains("waiting for the local cache workspace lock")
            .then_some(())
    });
    interrupt(&waiting);
    assert_eq!(waiting.finish(), 130);
    assert!(!dir.join("result").exists());
    drop(lock);

    let mut running = spawn();
    let pid = pid_file(&dir.join("child.pid"));
    interrupt(&running);
    assert_eq!(running.finish(), 130);
    assert_stopped(pid);
    assert!(!dir.join(".lazel/cache").exists());
}

#[test]
fn ctrl_c_lets_an_in_progress_cache_replacement_finish() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path();
    fs::create_dir(dir.join("out")).unwrap();
    fs::write(dir.join("input"), "source").unwrap();
    let files = (0..512).map(|i| format!("out/{i}")).collect::<Vec<_>>();
    for file in &files {
        fs::write(dir.join(file), "cached").unwrap();
    }
    fs::write(
        dir.join("lazel.toml"),
        format!(
            "
[projects.app]
dir = '.'
[projects.app.tasks.build]
command = 'echo ran'
cache = true
inputs = ['input']
outputs = {files:?}
[projects.app.tasks.final]
command = 'touch unexpected'
depends_on = ['app:build']
"
        ),
    )
    .unwrap();
    let runner = env!("CARGO_BIN_EXE_lazel");
    let cold = Command::new(runner)
        .current_dir(dir)
        .args(["run", "build"])
        .output()
        .unwrap();
    assert!(cold.status.success(), "{cold:?}");
    for file in &files {
        fs::write(dir.join(file), "previous").unwrap();
    }
    let mut run = Running(
        Command::new(runner)
            .current_dir(dir)
            .args(["run", "--concurrency=2", "final"])
            .spawn()
            .unwrap(),
    );
    let pending = dir.join(".lazel/restoring");
    wait_for(|| pending.is_dir().then_some(()));
    interrupt(&run);
    assert_eq!(run.finish(), 130);
    assert!(!pending.exists());
    assert!(!dir.join("unexpected").exists());
    for file in &files {
        assert_eq!(fs::read_to_string(dir.join(file)).unwrap(), "cached");
    }
}

fn interrupt(run: &Running) {
    assert_eq!(
        unsafe { libc::kill(run.0.id() as libc::pid_t, libc::SIGINT) },
        0
    );
}
