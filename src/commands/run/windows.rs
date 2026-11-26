use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::ExitStatusExt;
use std::process::ExitStatus;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{
    DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Console::{
    CTRL_BREAK_EVENT, CTRL_C_EVENT, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
    STD_OUTPUT_HANDLE, SetConsoleCtrlHandler,
};
use windows_sys::Win32::System::JobObjects::*;
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows_sys::Win32::System::Threading::*;

use super::Shell;
use crate::cache::TaskEnvironment;

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn on_interrupt(event: u32) -> i32 {
    if matches!(event, CTRL_C_EVENT | CTRL_BREAK_EVENT) {
        INTERRUPTED.store(true, Ordering::Relaxed);
        1
    } else {
        0
    }
}

pub(super) struct Interrupt;

impl Interrupt {
    pub(super) fn install() -> io::Result<Self> {
        INTERRUPTED.store(false, Ordering::Relaxed);
        unsafe {
            check(SetConsoleCtrlHandler(None, 0))?;
            check(SetConsoleCtrlHandler(Some(on_interrupt), 1))?;
        }
        Ok(Self)
    }
}

impl Drop for Interrupt {
    fn drop(&mut self) {
        unsafe { SetConsoleCtrlHandler(Some(on_interrupt), 0) };
    }
}

pub(super) fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::Relaxed)
}

pub(super) struct Process {
    process: OwnedHandle,
    job: OwnedHandle,
    termination_sent: bool,
}

impl Process {
    pub(super) fn spawn(shell: &Shell<'_>) -> io::Result<(Self, Option<[File; 2]>)> {
        let interactive = shell.task.interactive;
        let job = job()?;
        let (streams, output) = if interactive {
            (
                [
                    inherited(STD_INPUT_HANDLE)?,
                    inherited(STD_OUTPUT_HANDLE)?,
                    inherited(STD_ERROR_HANDLE)?,
                ],
                None,
            )
        } else {
            let (stdout, stdout_writer) = pipe()?;
            let (stderr, stderr_writer) = pipe()?;
            (
                [
                    duplicate(File::open(r"\\.\NUL")?.as_raw_handle())?,
                    stdout_writer,
                    stderr_writer,
                ],
                Some([stdout, stderr]),
            )
        };
        let handles = streams.each_ref().map(AsRawHandle::as_raw_handle);
        let jobs = [job.as_raw_handle()];
        let mut attributes = Attributes::new()?;
        attributes.set(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &handles)?;
        attributes.set(PROC_THREAD_ATTRIBUTE_JOB_LIST, &jobs)?;

        let executable = cmd_path()?;
        let line = format!("cmd.exe {} {}", Shell::CMD_SWITCHES, shell.quoted_recipe());
        let mut line = wide(OsStr::new(&line))?;
        let working_dir = wide(shell.task.dir.as_os_str())?;
        let environment = shell.environment.map(environment_block).transpose()?;
        let environment = environment
            .as_ref()
            .map_or(null(), |block| block.as_ptr().cast());

        let startup = STARTUPINFOEXW {
            StartupInfo: STARTUPINFOW {
                cb: size_of::<STARTUPINFOEXW>() as u32,
                dwFlags: STARTF_USESTDHANDLES,
                hStdInput: handles[0],
                hStdOutput: handles[1],
                hStdError: handles[2],
                ..STARTUPINFOW::default()
            },
            lpAttributeList: attributes.as_ptr(),
        };
        let mut info = PROCESS_INFORMATION::default();
        let mut flags = EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT;
        if !interactive {
            flags |= CREATE_NEW_PROCESS_GROUP;
        }
        // Both attribute arrays and all inherited handles live until process creation returns.
        unsafe {
            check(CreateProcessW(
                executable.as_ptr(),
                line.as_mut_ptr(),
                null(),
                null(),
                1,
                flags,
                environment,
                working_dir.as_ptr(),
                &startup.StartupInfo,
                &mut info,
            ))?;
            let process = OwnedHandle::from_raw_handle(info.hProcess);
            drop(OwnedHandle::from_raw_handle(info.hThread));
            Ok((
                Self {
                    process,
                    job,
                    termination_sent: false,
                },
                output,
            ))
        }
    }

    pub(super) fn interrupted(&mut self) -> bool {
        false
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let wait = unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) };
        match wait {
            WAIT_TIMEOUT => return Ok(None),
            WAIT_OBJECT_0 => {}
            _ => return Err(io::Error::last_os_error()),
        }
        let mut counts = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        unsafe {
            check(QueryInformationJobObject(
                self.job.as_raw_handle(),
                JobObjectBasicAccountingInformation,
                (&mut counts as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of_val(&counts) as u32,
                null_mut(),
            ))?;
        }
        if counts.ActiveProcesses > 0 {
            self.terminate_job()?;
            return Ok(None);
        }
        let mut code = 0;
        unsafe { check(GetExitCodeProcess(self.process.as_raw_handle(), &mut code))? };
        Ok(Some(ExitStatus::from_raw(code)))
    }

    pub(super) fn terminate(&mut self, _: bool) {
        let _ = self.terminate_job();
    }

    fn terminate_job(&mut self) -> io::Result<()> {
        if !self.termination_sent {
            unsafe { check(TerminateJobObject(self.job.as_raw_handle(), 130))? };
            self.termination_sent = true;
        }
        Ok(())
    }

    pub(super) fn reap(&mut self) {
        unsafe { WaitForSingleObject(self.process.as_raw_handle(), INFINITE) };
    }
}

fn job() -> io::Result<OwnedHandle> {
    unsafe {
        let raw = CreateJobObjectW(null(), null());
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = OwnedHandle::from_raw_handle(raw);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        check(SetInformationJobObject(
            raw,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of_val(&limits) as u32,
        ))?;
        Ok(job)
    }
}

fn pipe() -> io::Result<(File, OwnedHandle)> {
    let mut reader = null_mut();
    let mut writer = null_mut();
    unsafe {
        check(CreatePipe(&mut reader, &mut writer, null(), 0))?;
        let reader = File::from(OwnedHandle::from_raw_handle(reader));
        let writer = OwnedHandle::from_raw_handle(writer);
        Ok((reader, duplicate(writer.as_raw_handle())?))
    }
}

fn inherited(which: u32) -> io::Result<OwnedHandle> {
    let handle = unsafe { GetStdHandle(which) };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        let null = File::options().read(true).write(true).open(r"\\.\NUL")?;
        duplicate(null.as_raw_handle())
    } else {
        duplicate(handle)
    }
}

fn duplicate(handle: HANDLE) -> io::Result<OwnedHandle> {
    let mut inherited = null_mut();
    unsafe {
        let process = GetCurrentProcess();
        check(DuplicateHandle(
            process,
            handle,
            process,
            &mut inherited,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        ))?;
        Ok(OwnedHandle::from_raw_handle(inherited))
    }
}

struct Attributes(Vec<usize>);

impl Attributes {
    fn new() -> io::Result<Self> {
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut bytes);
            if bytes == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut buffer = vec![0_usize; bytes.div_ceil(size_of::<usize>())];
            check(InitializeProcThreadAttributeList(
                buffer.as_mut_ptr().cast(),
                2,
                0,
                &mut bytes,
            ))?;
            Ok(Self(buffer))
        }
    }

    fn as_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.0.as_mut_ptr().cast()
    }

    fn set<T>(&mut self, kind: u32, value: &T) -> io::Result<()> {
        unsafe {
            check(UpdateProcThreadAttribute(
                self.as_ptr(),
                0,
                kind as usize,
                (value as *const T).cast(),
                size_of::<T>(),
                null_mut(),
                null(),
            ))
        }
    }
}

impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.as_ptr()) };
    }
}

fn cmd_path() -> io::Result<Vec<u16>> {
    let mut path = vec![0; 260];
    loop {
        let length = unsafe { GetSystemDirectoryW(path.as_mut_ptr(), path.len() as u32) } as usize;
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        if length < path.len() {
            path.truncate(length);
            path.extend(OsStr::new("\\cmd.exe").encode_wide());
            path.push(0);
            return Ok(path);
        }
        path.resize(length + 1, 0);
    }
}

fn environment_block(environment: &TaskEnvironment) -> io::Result<Vec<u16>> {
    let mut block = Vec::new();
    for (name, value) in environment {
        if let Some(value) = value {
            block.extend(name.encode_utf16());
            block.push(u16::from(b'='));
            block.extend(wide(value)?);
        }
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut encoded = value.encode_wide().collect::<Vec<_>>();
    if encoded.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "process argument contains NUL",
        ));
    }
    encoded.push(0);
    Ok(encoded)
}

fn check(success: i32) -> io::Result<()> {
    if success == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
