use std::collections::BTreeMap;
use std::env;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::Command;

use anyhow::{Context, Result};

use crate::cache::{LocalCache, TaskEnvironment, check_output, fingerprint};
use crate::workspace::{Task, Workspace};

pub fn execute(selector: &str, no_cache: bool) -> Result<i32> {
    let cwd = env::current_dir().context("could not determine working directory")?;
    let workspace = Workspace::load(&cwd)?;
    let order = workspace.execution_order(selector)?;
    let local = order
        .iter()
        .any(|(_, task)| task.cache.is_some())
        .then(|| LocalCache::lock(&workspace.root))
        .transpose()?;
    let mut completed = BTreeMap::new();
    for (name, task) in order {
        let Some((local, config)) = local.as_ref().zip(task.cache.as_ref()) else {
            let code = run_task(name, task, None)?;
            if code != 0 {
                return Ok(code);
            }
            continue;
        };
        for output in &config.outputs {
            check_output(&workspace.root, &task.project.join(output))?;
        }
        let environment = (!no_cache).then(|| config.environment());
        let prerequisites = task
            .depends_on
            .iter()
            .map(|dependency| completed.get(dependency.as_str()).copied())
            .collect::<Option<Vec<_>>>();
        let make_key = || {
            environment
                .as_ref()
                .zip(prerequisites.as_ref())
                .map(|(environment, deps)| {
                    fingerprint(&workspace.root, name, task, config, environment, deps)
                })
                .transpose()
        };
        let key = match make_key() {
            Ok(key) => key,
            Err(error) => {
                eprintln!("lazel: could not fingerprint {name}: {error:#}");
                None
            }
        };
        if let Some(key) = &key {
            if let Some(result) = local.restore(task, &config.outputs, key)? {
                eprintln!("lazel: cache hit {name}");
                completed.insert(name, result);
                continue;
            }
            eprintln!("lazel: cache miss {name}");
        } else {
            eprintln!("lazel: cache bypass {name}");
        }

        let code = run_task(name, task, environment.as_ref())?;
        if code != 0 {
            return Ok(code);
        }
        if let Some(key) = key {
            match make_key() {
                Ok(Some(after)) if after == key => match local.store(task, &config.outputs, &key) {
                    Ok(result) => {
                        completed.insert(name, result);
                    }
                    Err(error) => eprintln!("lazel: could not cache {name}: {error:#}"),
                },
                _ => eprintln!("lazel: cache inputs changed for {name}; result not stored"),
            }
        }
    }
    Ok(0)
}

fn run_task(name: &str, task: &Task, environment: Option<&TaskEnvironment>) -> Result<i32> {
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

    if let Some(environment) = environment {
        shell.env_clear();
        shell.envs(
            environment
                .iter()
                .filter_map(|(name, value)| value.as_ref().map(|value| (name, value))),
        );
    }
    let status = shell
        .current_dir(&task.dir)
        .status()
        .with_context(|| format!("could not launch task '{name}' in '{}'", task.dir.display()))?;
    if status.success() {
        return Ok(0);
    }
    let code = status.code();
    #[cfg(unix)]
    let code = code.or_else(|| status.signal().map(|signal| 128 + signal));
    let code = code.unwrap_or(1);
    eprintln!("lazel: task '{name}' failed with exit code {code}");
    Ok(code)
}
