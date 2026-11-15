use std::env;
use std::os::unix::process::ExitStatusExt;
use std::process::Command;

use anyhow::{Context, Result};

use crate::workspace::Workspace;

pub fn execute(selector: &str) -> Result<u8> {
    let cwd = env::current_dir().context("could not determine working directory")?;
    let workspace = Workspace::load(&cwd)?;
    for (name, task) in workspace.execution_order(selector)? {
        eprintln!("lazel: running {name}");
        let status = Command::new("/bin/sh")
            .args(["-c", &task.command])
            .current_dir(&task.dir)
            .status()
            .with_context(|| {
                format!("could not launch task '{name}' in '{}'", task.dir.display())
            })?;

        if !status.success() {
            let code = status
                .code()
                .or_else(|| status.signal().map(|signal| 128 + signal))
                .and_then(|code| u8::try_from(code).ok())
                .unwrap_or(1);
            eprintln!("lazel: task '{name}' failed with exit code {code}");
            return Ok(code);
        }
    }
    Ok(0)
}
