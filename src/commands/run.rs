use std::env;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::Command;

use anyhow::{Context, Result};

use crate::workspace::Workspace;

pub fn execute(selector: &str) -> Result<i32> {
    let cwd = env::current_dir().context("could not determine working directory")?;
    let workspace = Workspace::load(&cwd)?;
    for (name, task) in workspace.execution_order(selector)? {
        eprintln!("lazel: running {name}");
        #[cfg(unix)]
        let mut shell = Command::new("/bin/sh");
        #[cfg(unix)]
        shell.args(["-c", &task.command]);
        #[cfg(windows)]
        let mut shell = Command::new("cmd.exe");
        #[cfg(windows)]
        // cmd /S strips the outer quotes and parses the recipe inside them.
        shell
            .args(["/D", "/V:OFF", "/S", "/C"])
            .raw_arg(format!("\"{}\"", task.command));

        let status = shell.current_dir(&task.dir).status().with_context(|| {
            format!("could not launch task '{name}' in '{}'", task.dir.display())
        })?;

        if !status.success() {
            let code = status.code();
            #[cfg(unix)]
            let code = code.or_else(|| status.signal().map(|signal| 128 + signal));
            let code = code.unwrap_or(1);
            eprintln!("lazel: task '{name}' failed with exit code {code}");
            return Ok(code);
        }
    }
    Ok(0)
}
