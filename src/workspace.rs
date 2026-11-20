use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail, ensure};
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
    pub depends_on: Vec<String>,
    pub cache: Option<CacheConfig>,
}

pub(crate) struct Workspace {
    pub root: PathBuf,
    tasks: BTreeMap<String, Task>,
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
                let cache = CacheConfig::new(task.cache, task.inputs, task.outputs, task.env)
                    .with_context(|| format!("task '{selector}'"))?;
                if let Some(cache) = &cache {
                    for output in &cache.outputs {
                        let output = relative.join(output);
                        cache::check_output(&root, &output)
                            .with_context(|| format!("task '{selector}'"))?;
                        let key = cache::path_key(&output);
                        for (other, owner) in &output_roots {
                            ensure!(
                                !key.starts_with(other) && !other.starts_with(&key),
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
                        depends_on: task.depends_on,
                        cache,
                    },
                );
            }
        }
        Ok(Self { root, tasks })
    }

    pub(crate) fn execution_order(&self, selector: &str) -> Result<Vec<(&str, &Task)>> {
        ensure!(
            selector
                .split_once(':')
                .is_some_and(|(project, task)| valid_name(project) && valid_name(task)),
            "invalid task selector '{selector}': expected project:task"
        );
        let (name, task) = self
            .tasks
            .get_key_value(selector)
            .with_context(|| format!("unknown task '{selector}'"))?;
        let mut visits = BTreeMap::new();
        let mut order = Vec::new();
        self.visit(name, task, &mut visits, &mut order)?;

        // Visit the requested task first to preserve its dependency order during preflight.
        let requested = order.len();
        for (name, task) in &self.tasks {
            self.visit(name, task, &mut visits, &mut order)?;
        }
        order.truncate(requested);
        Ok(order)
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
