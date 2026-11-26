use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail, ensure};
use glob::Pattern;
use serde::Deserialize;

use crate::cache::{self, CacheConfig};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    projects: BTreeMap<String, ProjectConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectConfig {
    dir: PathBuf,
    tasks: BTreeMap<String, TaskConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskConfig {
    command: String,
    #[serde(default)]
    interactive: bool,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    cache: bool,
    #[serde(default)]
    inputs: Vec<String>,
    #[serde(default)]
    outputs: Vec<PathBuf>,
    #[serde(default)]
    env: Vec<String>,
}

pub(crate) struct Task {
    pub dir: PathBuf,
    pub project: PathBuf,
    pub command: String,
    pub interactive: bool,
    pub depends_on: Vec<String>,
    pub outputs: Vec<PathBuf>,
    pub cache: Option<CacheConfig>,
}

pub(crate) struct Workspace {
    pub root: PathBuf,
    tasks: BTreeMap<String, Task>,
}

pub(crate) struct Plan<'a> {
    pub tasks: Vec<(&'a str, &'a Task)>,
    dependents: Vec<Vec<usize>>,
    remaining: Vec<usize>,
    pub ready: BTreeSet<usize>,
}

impl<'a> Plan<'a> {
    fn new(tasks: Vec<(&'a str, &'a Task)>) -> Self {
        let indices = tasks
            .iter()
            .enumerate()
            .map(|(index, (name, _))| (*name, index))
            .collect::<BTreeMap<_, _>>();
        let mut dependents = vec![Vec::new(); tasks.len()];
        let mut remaining = Vec::with_capacity(tasks.len());
        let mut ready = BTreeSet::new();
        for (index, (_, task)) in tasks.iter().enumerate() {
            for dependency in &task.depends_on {
                dependents[indices[dependency.as_str()]].push(index);
            }
            remaining.push(task.depends_on.len());
            if task.depends_on.is_empty() {
                ready.insert(index);
            }
        }
        Self {
            tasks,
            dependents,
            remaining,
            ready,
        }
    }

    pub(crate) fn complete(&mut self, index: usize) {
        for &dependent in &self.dependents[index] {
            self.remaining[dependent] -= 1;
            if self.remaining[dependent] == 0 {
                self.ready.insert(dependent);
            }
        }
    }

    pub(crate) fn check_parallel_outputs(&self, root: &Path) -> Result<()> {
        let mut claims = Vec::<(PathBuf, usize)>::new();
        for (index, (name, task)) in self.tasks.iter().enumerate() {
            for output in &task.outputs {
                let output = task.project.join(output);
                if task.cache.is_none() {
                    cache::check_output(root, &output).with_context(|| format!("task '{name}'"))?;
                }
                let key = cache::path_key(&output);
                if !task.interactive {
                    for (other, owner) in &claims {
                        ensure!(
                            *owner == index
                                || !roots_overlap(&key, other)
                                || self.is_prerequisite(*owner, index),
                            "overlapping parallel output '{}' in '{name}' and '{}'",
                            output.display(),
                            self.tasks[*owner].0
                        );
                    }
                    claims.push((key, index));
                }
            }
        }
        Ok(())
    }

    fn is_prerequisite(&self, from: usize, to: usize) -> bool {
        let mut pending = vec![from];
        let mut seen = BTreeSet::new();
        while let Some(index) = pending.pop() {
            for &dependent in &self.dependents[index] {
                if dependent == to {
                    return true;
                }
                if dependent < to && seen.insert(dependent) {
                    pending.push(dependent);
                }
            }
        }
        false
    }
}

fn roots_overlap(first: &Path, second: &Path) -> bool {
    first.starts_with(second) || second.starts_with(first)
}

enum Visit {
    Visiting,
    Visited,
}

impl Workspace {
    pub(crate) fn load(start: &Path) -> Result<Self> {
        let root = find_root(start)?;
        let config_path = root.join("lazel.toml");
        let read_context = || {
            format!(
                "could not read workspace config '{}'",
                config_path.display()
            )
        };
        ensure!(
            fs::metadata(&config_path)
                .with_context(read_context)?
                .is_file(),
            "invalid workspace config '{}': expected a file",
            config_path.display()
        );
        let contents = fs::read_to_string(&config_path).with_context(read_context)?;
        let config: Config = toml::from_str(&contents).map_err(|error| {
            anyhow!(
                "invalid workspace config '{}': {}",
                config_path.display(),
                error.message()
            )
        })?;

        let mut tasks = BTreeMap::new();
        let mut output_roots: Vec<(PathBuf, String)> = Vec::new();
        for (project, config) in config.projects {
            ensure!(valid_name(&project), "invalid project name '{project}'");
            ensure!(
                config.dir.is_relative() && !config.dir.as_os_str().is_empty(),
                "project directory for '{project}' must be relative and nonempty"
            );
            let dir = root.join(&config.dir).canonicalize().with_context(|| {
                format!(
                    "could not resolve project directory '{}' for '{project}'",
                    config.dir.display()
                )
            })?;
            ensure!(
                dir.starts_with(&root),
                "project directory for '{project}' is outside the workspace: '{}'",
                dir.display()
            );
            ensure!(
                dir.is_dir(),
                "project directory for '{project}' is not a directory: '{}'",
                dir.display()
            );
            let relative = dir.strip_prefix(&root)?.to_path_buf();
            #[cfg(windows)]
            let dir = cmd_directory(&dir).with_context(|| format!("project '{project}'"))?;

            for (name, task) in config.tasks {
                let selector = format!("{project}:{name}");
                ensure!(valid_name(&name), "invalid task name in '{selector}'");
                ensure!(
                    !task.command.trim().is_empty(),
                    "blank command for task '{selector}'"
                );
                ensure!(
                    !task.interactive || !task.cache,
                    "interactive task '{selector}' cannot enable caching"
                );
                let cache = CacheConfig::new(task.cache, task.inputs, task.env)
                    .with_context(|| format!("task '{selector}'"))?;
                let outputs = cache::normalize_outputs(task.outputs)
                    .with_context(|| format!("task '{selector}'"))?;
                if cache.is_some() {
                    for output in &outputs {
                        let output = relative.join(output);
                        cache::check_output(&root, &output)
                            .with_context(|| format!("task '{selector}'"))?;
                        let key = cache::path_key(&output);
                        for (other, owner) in &output_roots {
                            ensure!(
                                !roots_overlap(&key, other),
                                "overlapping cache output '{}' in '{selector}' and '{owner}'",
                                output.display()
                            );
                        }
                        output_roots.push((key, selector.clone()));
                    }
                }
                tasks.insert(
                    selector,
                    Task {
                        dir: dir.clone(),
                        project: relative.clone(),
                        command: task.command,
                        interactive: task.interactive,
                        depends_on: task.depends_on,
                        outputs,
                        cache,
                    },
                );
            }
        }
        Ok(Self { root, tasks })
    }

    pub(crate) fn plan(&self, selectors: &[String], filter: Option<&str>) -> Result<Plan<'_>> {
        let filter = filter
            .map(|pattern| {
                Pattern::new(pattern).with_context(|| format!("invalid project filter '{pattern}'"))
            })
            .transpose()?;
        let matches_filter = |project: &str| filter.as_ref().is_none_or(|p| p.matches(project));
        let mut roots = Vec::new();
        for selector in selectors {
            if let Some((project, task)) = selector.split_once(':') {
                ensure!(
                    valid_name(project) && valid_name(task),
                    "invalid task selector '{selector}': expected task or project:task"
                );
                let (name, task) = self
                    .tasks
                    .get_key_value(selector)
                    .with_context(|| format!("unknown task '{selector}'"))?;
                ensure!(
                    matches_filter(project),
                    "task '{selector}' does not match the project filter"
                );
                roots.push((name, task));
            } else {
                ensure!(
                    valid_name(selector),
                    "invalid task selector '{selector}': expected task or project:task"
                );
                let previous = roots.len();
                roots.extend(self.tasks.iter().filter(|(name, _)| {
                    name.split_once(':')
                        .is_some_and(|(project, task)| task == selector && matches_filter(project))
                }));
                ensure!(roots.len() > previous, "no tasks match '{selector}'");
                roots[previous..].sort_unstable_by_key(|(name, _)| name.split(':').next());
            }
        }
        let mut visits = BTreeMap::new();
        let mut order = Vec::new();
        for (name, task) in roots {
            self.visit(name, task, &mut visits, &mut order)?;
        }

        // Keep the selected traversal order while validating the rest of the workspace.
        let requested = order.len();
        for (name, task) in &self.tasks {
            self.visit(name, task, &mut visits, &mut order)?;
        }
        order.truncate(requested);
        Ok(Plan::new(order))
    }

    fn visit<'a>(
        &'a self,
        root: &'a str,
        task: &'a Task,
        visits: &mut BTreeMap<&'a str, Visit>,
        order: &mut Vec<(&'a str, &'a Task)>,
    ) -> Result<()> {
        if visits.contains_key(root) {
            return Ok(());
        }
        visits.insert(root, Visit::Visiting);
        let mut stack = vec![(root, task, task.depends_on.iter())];

        while let Some((name, task, dependencies)) = stack.last_mut() {
            let Some(dependency) = dependencies.next().map(String::as_str) else {
                visits.insert(*name, Visit::Visited);
                order.push((*name, *task));
                stack.pop();
                continue;
            };

            match visits.get(dependency) {
                Some(Visit::Visited) => {}
                Some(Visit::Visiting) => {
                    let cycle = stack
                        .iter()
                        .map(|(name, _, _)| *name)
                        .skip_while(|name| *name != dependency)
                        .chain(std::iter::once(dependency))
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    bail!("dependency cycle: {cycle}");
                }
                None => {
                    let task = self.tasks.get(dependency).with_context(|| {
                        format!("unknown dependency '{dependency}' for task '{name}'")
                    })?;
                    visits.insert(dependency, Visit::Visiting);
                    stack.push((dependency, task, task.depends_on.iter()));
                }
            }
        }
        Ok(())
    }
}

fn find_root(start: &Path) -> Result<PathBuf> {
    for dir in start.ancestors() {
        let path = dir.join("lazel.toml");
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                return dir.canonicalize().with_context(|| {
                    format!("could not resolve workspace directory '{}'", dir.display())
                });
            }
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("could not inspect workspace config '{}'", path.display())
                });
            }
        }
    }
    bail!(
        "no lazel.toml found from '{}' to the filesystem root",
        start.display()
    );
}

#[cfg(windows)]
fn cmd_directory(dir: &Path) -> Result<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Component, Prefix};

    let Some(Component::Prefix(prefix)) = dir.components().next() else {
        bail!(
            "expected an absolute Windows project directory: '{}'",
            dir.display()
        );
    };
    match prefix.kind() {
        Prefix::Disk(_) => Ok(dir.to_path_buf()),
        Prefix::VerbatimDisk(_) => {
            // cmd treats the verbatim prefix as a network path and changes the working directory.
            let wide_path: Vec<_> = dir.as_os_str().encode_wide().skip(4).collect();
            let plain = PathBuf::from(OsString::from_wide(&wide_path));
            ensure!(
                plain.canonicalize().ok().as_deref() == Some(dir),
                "cmd.exe cannot use project directory '{}'",
                dir.display()
            );
            Ok(plain)
        }
        _ => bail!(
            "cmd.exe requires a local drive project directory: '{}'",
            dir.display()
        ),
    }
}

fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

#[cfg(test)]
#[path = "../tests/unit/workspace.rs"]
mod tests;
