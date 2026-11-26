use std::fs::File;
use std::io::{self, IsTerminal, Read};
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::Shell;

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_interrupt(_: libc::c_int) {
    INTERRUPTED.store(true, Ordering::Relaxed);
}

pub(super) fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::Relaxed)
}

pub(super) struct Interrupt(libc::sigaction);

impl Interrupt {
    pub(super) fn install() -> io::Result<Self> {
        INTERRUPTED.store(false, Ordering::Relaxed);
        // sigaction and its mask are initialized before installing the handler.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_interrupt as *const () as usize;
            libc::sigemptyset(&mut action.sa_mask);
            let mut prior = MaybeUninit::uninit();
            check(libc::sigaction(libc::SIGINT, &action, prior.as_mut_ptr()))?;
            Ok(Self(prior.assume_init()))
        }
    }
}

impl Drop for Interrupt {
    fn drop(&mut self) {
        unsafe { libc::sigaction(libc::SIGINT, &self.0, std::ptr::null_mut()) };
    }
}

pub(super) struct Process {
    child: Child,
    group: libc::pid_t,
    foreground: Option<Foreground>,
    settled: Arc<AtomicBool>,
    reaped: bool,
}

impl Process {
    pub(super) fn spawn(shell: &Shell<'_>) -> io::Result<(Self, Option<[TaskPipe; 2]>)> {
        let interactive = shell.task.interactive;
        let mut command = shell.command();
        let foreground = if interactive && io::stdin().is_terminal() {
            Foreground::acquire()?
        } else {
            None
        };
        if !interactive {
            command
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
        }
        let group = foreground.as_ref().map_or(0, |foreground| foreground.relay);
        let child = command.process_group(group).spawn()?;
        let group = if group == 0 {
            child.id() as libc::pid_t
        } else {
            group
        };
        let mut process = Self {
            child,
            group,
            foreground,
            settled: Arc::new(AtomicBool::new(false)),
            reaped: false,
        };
        let pipes = if interactive {
            None
        } else {
            let stdout = process
                .child
                .stdout
                .take()
                .ok_or_else(|| io::Error::other("missing stdout pipe"))?;
            let stderr = process
                .child
                .stderr
                .take()
                .ok_or_else(|| io::Error::other("missing stderr pipe"))?;
            Some([
                TaskPipe::new(stdout.into(), Arc::clone(&process.settled))?,
                TaskPipe::new(stderr.into(), Arc::clone(&process.settled))?,
            ])
        };
        Ok((process, pipes))
    }

    pub(super) fn interrupted(&mut self) -> bool {
        self.foreground
            .as_mut()
            .is_some_and(Foreground::interrupted)
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // Inspect status without reaping the leader, which still owns the group ID.
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                self.child.id(),
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        match check(result) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => return Ok(None),
            rest => rest?,
        }
        if unsafe { info.si_pid() } == 0 {
            return Ok(None);
        }
        if let Some(foreground) = self.foreground.as_mut() {
            foreground.finish()?;
        }
        self.terminate(true);
        self.wait().map(Some)
    }

    pub(super) fn terminate(&mut self, force: bool) {
        if !self.reaped {
            let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
            // The group is owned until the shell and optional relay have been waited for.
            unsafe { libc::killpg(self.group, signal) };
        }
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        let status = self.child.wait()?;
        self.reaped = true;
        self.foreground.take();
        self.settled.store(true, Ordering::Relaxed);
        Ok(status)
    }

    pub(super) fn reap(&mut self) {
        if !self.reaped {
            let _ = self.wait();
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.terminate(true);
        self.reap();
    }
}

pub(super) struct TaskPipe {
    file: File,
    settled: Arc<AtomicBool>,
    drain_remaining: Option<usize>,
}

impl TaskPipe {
    fn new(fd: OwnedFd, settled: Arc<AtomicBool>) -> io::Result<Self> {
        let raw = fd.as_raw_fd();
        let flags = unsafe { libc::fcntl(raw, libc::F_GETFL) };
        check(flags)?;
        check(unsafe { libc::fcntl(raw, libc::F_SETFL, flags | libc::O_NONBLOCK) })?;
        Ok(Self {
            file: fd.into(),
            settled,
            drain_remaining: None,
        })
    }
}

impl Read for TaskPipe {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.drain_remaining.is_none() && self.settled.load(Ordering::Relaxed) {
                // Drain the queued bytes without waiting on a writer outside the managed group.
                let mut available: libc::c_int = 0;
                check(unsafe {
                    libc::ioctl(self.file.as_raw_fd(), libc::FIONREAD as _, &mut available)
                })?;
                self.drain_remaining = Some(available as usize);
            }
            let limit = buffer.len().min(self.drain_remaining.unwrap_or(usize::MAX));
            match self.file.read(&mut buffer[..limit]) {
                Ok(count) => {
                    if let Some(remaining) = &mut self.drain_remaining {
                        *remaining -= count;
                    }
                    return Ok(count);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if self.drain_remaining.is_some() {
                        return Ok(0);
                    }
                    let mut poll = libc::pollfd {
                        fd: self.file.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    match check(unsafe { libc::poll(&mut poll, 1, 25) }) {
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                        rest => rest?,
                    }
                }
                result => return result,
            }
        }
    }
}

struct Foreground {
    tty: File,
    prior: libc::pid_t,
    relay: libc::pid_t,
    notices: UnixStream,
    owns_terminal: bool,
}

impl Foreground {
    fn acquire() -> io::Result<Option<Self>> {
        let tty = match File::open("/dev/tty") {
            Ok(tty) => tty,
            Err(error) if error.raw_os_error() == Some(libc::ENXIO) => return Ok(None),
            Err(error) => return Err(error),
        };
        let prior = unsafe { libc::tcgetpgrp(tty.as_raw_fd()) };
        check(prior)?;
        if prior != unsafe { libc::getpgrp() } {
            return Err(io::Error::other(
                "Lazel does not own the foreground terminal",
            ));
        }
        let (notices, child_socket) = UnixStream::pair()?;
        notices.set_read_timeout(Some(Duration::from_secs(2)))?;
        let relay = unsafe { libc::fork() };
        check(relay)?;
        if relay == 0 {
            // The forked relay uses only async-signal-safe calls before _exit.
            unsafe { relay_interrupts(notices.as_raw_fd(), child_socket.as_raw_fd()) }
        }
        drop(child_socket);
        let mut foreground = Self {
            tty,
            prior,
            relay,
            notices,
            owns_terminal: false,
        };
        let mut ready = [0];
        foreground.notices.read_exact(&mut ready)?;
        if ready != *b"R" {
            return Err(io::Error::other("terminal interrupt relay could not start"));
        }
        foreground.notices.set_nonblocking(true)?;
        terminal_group(foreground.tty.as_raw_fd(), relay)?;
        foreground.owns_terminal = true;
        Ok(Some(foreground))
    }

    fn interrupted(&mut self) -> bool {
        let mut byte = [0];
        if self.notices.read(&mut byte).is_ok_and(|n| n == 1) && byte == *b"I" {
            INTERRUPTED.store(true, Ordering::Relaxed);
            return true;
        }
        false
    }

    fn finish(&mut self) -> io::Result<()> {
        self.restore()?;
        // Restore first so new terminal interrupts reach Lazel while the relay reports pending ones.
        unsafe { libc::kill(self.relay, libc::SIGUSR1) };
        self.notices.set_nonblocking(false)?;
        self.interrupted();
        Ok(())
    }

    fn restore(&mut self) -> io::Result<()> {
        if self.owns_terminal {
            terminal_group(self.tty.as_raw_fd(), self.prior)?;
            self.owns_terminal = false;
        }
        Ok(())
    }
}

impl Drop for Foreground {
    fn drop(&mut self) {
        let _ = self.restore();
        unsafe {
            libc::kill(self.relay, libc::SIGKILL);
            while libc::waitpid(self.relay, std::ptr::null_mut(), 0) == -1
                && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted
            {}
        }
    }
}

unsafe fn relay_interrupts(parent: RawFd, socket: RawFd) -> ! {
    unsafe {
        libc::close(parent);
        let mut signals = std::mem::zeroed();
        libc::sigemptyset(&mut signals);
        libc::sigaddset(&mut signals, libc::SIGINT);
        libc::sigaddset(&mut signals, libc::SIGUSR1);
        if libc::setpgid(0, 0) < 0
            || libc::pthread_sigmask(libc::SIG_BLOCK, &signals, std::ptr::null_mut()) != 0
            || libc::write(socket, b"R".as_ptr().cast(), 1) != 1
        {
            libc::_exit(1);
        }
        let mut signal = 0;
        if libc::sigwait(&signals, &mut signal) == 0 {
            let pending = libc::sigpending(&mut signals) == 0
                && libc::sigismember(&signals, libc::SIGINT) == 1;
            let notice = if signal == libc::SIGINT || pending {
                b"I"
            } else {
                b"D"
            };
            libc::write(socket, notice.as_ptr().cast(), 1);
        }
        libc::_exit(0);
    }
}

fn terminal_group(tty: RawFd, group: libc::pid_t) -> io::Result<()> {
    unsafe {
        let mut signals = std::mem::zeroed();
        let mut prior = std::mem::zeroed();
        libc::sigemptyset(&mut signals);
        libc::sigaddset(&mut signals, libc::SIGTTOU);
        let result = libc::pthread_sigmask(libc::SIG_BLOCK, &signals, &mut prior);
        if result != 0 {
            return Err(io::Error::from_raw_os_error(result));
        }
        let result = check(libc::tcsetpgrp(tty, group));
        libc::pthread_sigmask(libc::SIG_SETMASK, &prior, std::ptr::null_mut());
        result
    }
}

fn check(result: libc::c_int) -> io::Result<()> {
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
