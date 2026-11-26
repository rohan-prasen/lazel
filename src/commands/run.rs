use std::collections::BTreeMap;
use std::env;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::cache::{Fingerprint, LocalCache, TaskEnvironment, check_output, fingerprint};
use crate::workspace::{Task, Workspace};

#[cfg_attr(unix, path = "run/unix.rs")]
#[cfg_attr(windows, path = "run/windows.rs")]
mod platform;
mod streams;

pub fn execute(
    selectors: &[String],
    filter: Option<&str>,
    concurrency: usize,
    no_cache: bool,
) -> Result<i32> {
    let cwd = env::current_dir().context("could not determine working directory")?;
    let workspace = Workspace::load(&cwd)?;
    let mut plan = workspace.plan(selectors, filter)?;
    if concurrency > 1 {
        plan.check_parallel_outputs(&workspace.root)?;
    }
    let _interrupt = (concurrency > 1)
        .then(platform::Interrupt::install)
        .transpose()?;
    let local = plan
        .tasks
        .iter()
        .any(|(_, task)| task.cache.is_some())
        .then(|| {
            LocalCache::lock(
                &workspace.root,
                (concurrency > 1).then_some(platform::interrupted),
            )
        })
        .transpose();
    if platform::interrupted() {
        return Ok(130);
    }
    let mut cache = RunCache {
        root: &workspace.root,
        local: local?,
        completed: BTreeMap::new(),
        no_cache,
    };
    let mut active = BTreeMap::<usize, ActiveRecipe>::new();
    let mut failure = None;
    let mut canceled = None;
    loop {
        if canceled.is_none()
            && (platform::interrupted()
                || active.values_mut().any(|recipe| recipe.child.interrupted()))
        {
            canceled = Some(Instant::now());
            for recipe in active.values_mut() {
                recipe.child.terminate(false);
            }
        }
        if canceled.is_some_and(|start| start.elapsed() >= Duration::from_millis(250)) {
            for recipe in active.values_mut() {
                recipe.child.terminate(true);
            }
        }
        let mut completed = Vec::new();
        for (&index, recipe) in &mut active {
            let (name, _) = plan.tasks[index];
            if recipe.code.is_none() {
                match recipe.child.try_wait() {
                    Ok(Some(status)) => {
                        let code = report_status(name, status);
                        if code != 0 {
                            failure.get_or_insert(Ok(code));
                        }
                        recipe.code = Some(code);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        recipe.child.terminate(true);
                        recipe.child.reap();
                        recipe.code = Some(1);
                        failure.get_or_insert_with(|| {
                            Err(error).with_context(|| format!("could not wait for task '{name}'"))
                        });
                    }
                }
            }
            if recipe.code.is_some() && recipe.readers.iter().all(|reader| reader.is_finished()) {
                completed.push(index);
            }
        }
        for index in completed {
            let (name, task) = plan.tasks[index];
            let recipe = active.remove(&index).expect("completed task is active");
            if let Err(error) = streams::finish(recipe.readers) {
                failure.get_or_insert_with(|| {
                    Err(error)
                        .with_context(|| format!("could not stream output from task '{name}'"))
                });
            } else if recipe.code == Some(0) && canceled.is_none() && !platform::interrupted() {
                cache.finish(name, task, recipe.prepared);
                plan.complete(index);
            }
        }

        if platform::interrupted() && canceled.is_none() {
            continue;
        }
        if failure.is_none()
            && canceled.is_none()
            && active.len() < concurrency
            && let Some(&index) = plan.ready.first()
            && (active.is_empty()
                || (!plan.tasks[index].1.interactive
                    && !active
                        .keys()
                        .any(|&running| plan.tasks[running].1.interactive)))
        {
            plan.ready.remove(&index);
            let (name, task) = plan.tasks[index];
            let recipe = match cache.prepare(name, task) {
                Ok(Some(recipe)) => recipe,
                Ok(None) => {
                    plan.complete(index);
                    continue;
                }
                Err(error) => {
                    failure = Some(Err(error));
                    continue;
                }
            };
            if platform::interrupted() {
                continue;
            }
            eprintln!("lazel: running {name}");
            if concurrency == 1 {
                let status = Shell {
                    task,
                    environment: recipe.environment.as_ref(),
                }
                .command()
                .status()
                .with_context(|| launch_error(name, task))?;
                let code = report_status(name, status);
                if code != 0 {
                    return Ok(code);
                }
                cache.finish(name, task, recipe);
                plan.complete(index);
            } else {
                match ActiveRecipe::spawn(name, task, recipe)
                    .with_context(|| launch_error(name, task))
                {
                    Ok(recipe) => {
                        active.insert(index, recipe);
                    }
                    Err(error) => failure = Some(Err(error)),
                }
            }
        } else if active.is_empty() {
            if canceled.is_some() {
                return Ok(130);
            }
            return failure.unwrap_or(Ok(0));
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }
}

struct RunCache<'a> {
    root: &'a Path,
    local: Option<LocalCache<'a>>,
    completed: BTreeMap<&'a str, Fingerprint>,
    no_cache: bool,
}

struct ActiveRecipe {
    child: platform::Process,
    prepared: PreparedRecipe,
    code: Option<i32>,
    readers: Vec<streams::Reader>,
}

impl ActiveRecipe {
    fn spawn(name: &str, task: &Task, prepared: PreparedRecipe) -> Result<Self> {
        let shell = Shell {
            task,
            environment: prepared.environment.as_ref(),
        };
        let (mut child, pipes) = platform::Process::spawn(&shell)?;
        let mut readers = Vec::with_capacity(2);
        let attach = (|| {
            if let Some([stdout, stderr]) = pipes {
                readers.push(streams::start(name, stdout, false)?);
                readers.push(streams::start(name, stderr, true)?);
            }
            Ok(())
        })();
        if let Err(error) = attach {
            child.terminate(true);
            child.reap();
            let _ = streams::finish(readers);
            return Err(error);
        }
        Ok(Self {
            child,
            prepared,
            code: None,
            readers,
        })
    }
}

#[derive(Default)]
struct PreparedRecipe {
    environment: Option<TaskEnvironment>,
    miss: Option<CacheMiss>,
}

struct CacheMiss {
    key: Fingerprint,
    prerequisites: Vec<Fingerprint>,
}

impl<'a> RunCache<'a> {
    fn prepare(&mut self, name: &'a str, task: &Task) -> Result<Option<PreparedRecipe>> {
        let Some((local, config)) = self.local.as_ref().zip(task.cache.as_ref()) else {
            return Ok(Some(PreparedRecipe::default()));
        };
        for output in &task.outputs {
            check_output(self.root, &task.project.join(output))?;
        }
        let environment = (!self.no_cache).then(|| config.environment());
        let miss = environment.as_ref().and_then(|environment| {
            let prerequisites = task
                .depends_on
                .iter()
                .map(|dependency| self.completed.get(dependency.as_str()).copied())
                .collect::<Option<Vec<_>>>()?;
            match fingerprint(self.root, name, task, config, environment, &prerequisites) {
                Ok(key) => Some(CacheMiss { key, prerequisites }),
                Err(error) => {
                    eprintln!("lazel: could not fingerprint {name}: {error:#}");
                    None
                }
            }
        });
        if let Some(miss) = &miss {
            if let Some(result) = local.restore(task, &task.outputs, &miss.key)? {
                eprintln!("lazel: cache hit {name}");
                self.completed.insert(name, result);
                return Ok(None);
            }
            eprintln!("lazel: cache miss {name}");
        } else {
            eprintln!("lazel: cache bypass {name}");
        }
        Ok(Some(PreparedRecipe { environment, miss }))
    }

    fn finish(&mut self, name: &'a str, task: &Task, recipe: PreparedRecipe) {
        let (Some(local), Some(config), Some(environment), Some(miss)) =
            (&self.local, &task.cache, recipe.environment, recipe.miss)
        else {
            return;
        };
        let result = fingerprint(
            self.root,
            name,
            task,
            config,
            &environment,
            &miss.prerequisites,
        );
        if result.is_ok_and(|after| after == miss.key) {
            match local.store(task, &task.outputs, &miss.key) {
                Ok(result) => {
                    self.completed.insert(name, result);
                }
                Err(error) => eprintln!("lazel: could not cache {name}: {error:#}"),
            }
        } else {
            eprintln!("lazel: cache inputs changed for {name}; result not stored");
        }
    }
}

struct Shell<'a> {
    task: &'a Task,
    environment: Option<&'a TaskEnvironment>,
}

impl Shell<'_> {
    #[cfg(windows)]
    const CMD_SWITCHES: &str = "/D /V:OFF /S /C";

    #[cfg(windows)]
    fn quoted_recipe(&self) -> String {
        // cmd /S strips the outer quotes and parses the recipe inside them.
        format!("\"{}\"", self.task.command)
    }

    fn command(&self) -> Command {
        #[cfg(unix)]
        let mut shell = Command::new("/bin/sh");
        #[cfg(unix)]
        shell.args(["-c", &self.task.command]);
        #[cfg(windows)]
        let mut shell = Command::new("cmd.exe");
        #[cfg(windows)]
        shell
            .args(Self::CMD_SWITCHES.split_ascii_whitespace())
            .raw_arg(self.quoted_recipe());

        if let Some(environment) = self.environment {
            shell.env_clear();
            shell.envs(
                environment
                    .iter()
                    .filter_map(|(name, value)| value.as_ref().map(|value| (name, value))),
            );
        }
        shell.current_dir(&self.task.dir);
        shell
    }
}

fn launch_error(name: &str, task: &Task) -> String {
    format!("could not launch task '{name}' in '{}'", task.dir.display())
}

fn report_status(name: &str, status: ExitStatus) -> i32 {
    if status.success() {
        return 0;
    }
    let code = status.code();
    #[cfg(unix)]
    let code = code.or_else(|| status.signal().map(|signal| 128 + signal));
    let code = code.unwrap_or(1);
    eprintln!("lazel: task '{name}' failed with exit code {code}");
    code
}
