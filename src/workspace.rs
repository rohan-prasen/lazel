use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::Deserialize;

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
}

pub(crate) struct Task {
    pub dir: PathBuf,
    pub command: String,
    depends_on: Vec<String>,
}

pub(crate) struct Workspace {
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

            for (name, task) in config.tasks {
                let selector = format!("{project}:{name}");
                ensure!(valid_name(&name), "invalid task name in '{selector}'");
                ensure!(
                    !task.command.trim().is_empty(),
                    "blank command for task '{selector}'"
                );
                tasks.insert(
                    selector,
                    Task {
                        dir: dir.clone(),
                        command: task.command,
                        depends_on: task.depends_on,
                    },
                );
            }
        }
        Ok(Self { tasks })
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

fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}
