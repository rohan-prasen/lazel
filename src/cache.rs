use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fs::{self, File, Metadata};
use std::io::{BufReader, ErrorKind, Read};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, ensure};
use glob::{MatchOptions, Pattern};
use sha2::{Digest, Sha256};

use crate::workspace::Task;

mod store;
pub(crate) use store::LocalCache;

pub(crate) struct CacheConfig {
    inputs: Vec<Pattern>,
    env: Vec<String>,
}

pub(crate) type TaskEnvironment = BTreeMap<String, Option<OsString>>;
pub(crate) type Fingerprint = [u8; 32];

impl CacheConfig {
    pub fn new(enabled: bool, inputs: Vec<String>, env: Vec<String>) -> Result<Option<Self>> {
        ensure!(
            !enabled || !inputs.is_empty(),
            "cacheable task requires inputs"
        );
        let inputs = inputs
            .iter()
            .map(|input| {
                ensure!(
                    !input.contains(['\\', ':', '\0'])
                        && input
                            .split('/')
                            .all(|part| !matches!(part, "" | "." | "..")),
                    "expected a project-relative input glob, got '{input}'"
                );
                ensure!(!reserved(Path::new(input)), "reserved input path '{input}'");
                Pattern::new(input).with_context(|| format!("invalid input glob '{input}'"))
            })
            .collect::<Result<Vec<_>>>()?;
        for name in &env {
            let mut chars = name.bytes();
            ensure!(
                chars
                    .next()
                    .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
                    && chars.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'),
                "invalid environment variable name '{name}'"
            );
        }
        Ok(enabled.then_some(Self { inputs, env }))
    }

    pub fn environment(&self) -> TaskEnvironment {
        #[cfg(windows)]
        let defaults = ["PATH", "SYSTEMROOT", "PATHEXT"];
        #[cfg(not(windows))]
        let defaults = ["PATH"];

        defaults
            .into_iter()
            .chain(self.env.iter().map(String::as_str))
            .map(|name| {
                let name = if cfg!(windows) {
                    name.to_ascii_uppercase()
                } else {
                    name.to_owned()
                };
                let value = env::var_os(&name);
                (name, value)
            })
            .collect()
    }
}

pub(crate) fn fingerprint(
    root: &Path,
    selector: &str,
    task: &Task,
    config: &CacheConfig,
    environment: &TaskEnvironment,
    dependencies: &[Fingerprint],
) -> Result<Fingerprint> {
    let mut hash = Sha256::new();
    for value in [
        "lazel-task-v1",
        env::consts::OS,
        env::consts::ARCH,
        selector,
        &task.command,
    ] {
        field(&mut hash, value.as_bytes());
    }
    field(&mut hash, task.project.as_os_str().as_encoded_bytes());
    field(&mut hash, &config.inputs.len().to_le_bytes());
    for pattern in &config.inputs {
        field(&mut hash, pattern.as_str().as_bytes());
    }
    field(&mut hash, &task.outputs.len().to_le_bytes());
    for output in &task.outputs {
        field(&mut hash, output.as_os_str().as_encoded_bytes());
    }
    hash_environment(&mut hash, environment);
    field(&mut hash, &dependencies.len().to_le_bytes());
    for result in dependencies {
        field(&mut hash, result);
    }

    let inputs = input_files(root, task, config)?;
    field(&mut hash, &inputs.len().to_le_bytes());
    for input in inputs {
        let relative = task.project.join(&input);
        let metadata = checked_metadata(root, &relative)?
            .filter(Metadata::is_file)
            .with_context(|| format!("cache input '{}' disappeared", input.display()))?;
        field(&mut hash, input.as_os_str().as_encoded_bytes());
        field(&mut hash, &file_mode(&metadata).to_le_bytes());
        field(&mut hash, &file_digest(&root.join(relative))?);
    }
    Ok(hash.finalize().into())
}

fn hash_environment(hash: &mut Sha256, environment: &TaskEnvironment) {
    field(hash, &environment.len().to_le_bytes());
    for (name, value) in environment {
        field(hash, name.as_bytes());
        field(hash, &[u8::from(value.is_some())]);
        if let Some(value) = value {
            field(hash, value.as_encoded_bytes());
        }
    }
}

fn field(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_le_bytes());
    hash.update(value);
}

fn input_files(root: &Path, task: &Task, cache: &CacheConfig) -> Result<BTreeSet<PathBuf>> {
    let mut inputs = BTreeSet::new();
    let options = MatchOptions {
        case_sensitive: !cfg!(windows),
        require_literal_separator: true,
        require_literal_leading_dot: false,
    };
    let outputs: Vec<_> = task.outputs.iter().map(|path| path_key(path)).collect();
    for pattern in &cache.inputs {
        let parts = pattern.as_str().split('/');
        let prefix: PathBuf = parts
            .clone()
            .take_while(|part| !part.contains(['*', '?', '[']))
            .collect();
        let depth = if parts.clone().any(|part| part == "**") {
            usize::MAX
        } else {
            parts.count()
        };
        let mut pending = vec![prefix];
        while let Some(path) = pending.pop() {
            let relative = task.project.join(&path);
            let key = path_key(&path);
            if reserved(&relative) || outputs.iter().any(|output| key.starts_with(output)) {
                continue;
            }
            let metadata = if relative.as_os_str().is_empty() {
                plain_metadata(root)?
            } else {
                checked_metadata(root, &relative)?
            };
            let Some(metadata) = metadata else { continue };
            if metadata.is_dir() && path.components().count() < depth {
                for child in root.join(&relative).read_dir()? {
                    pending.push(path.join(child?.file_name()));
                }
            } else if metadata.is_file() {
                ensure!(path.to_str().is_some(), "cache input path is not UTF-8");
                if pattern.matches_path_with(&path, options) {
                    inputs.insert(path);
                }
            }
        }
    }
    Ok(inputs)
}

fn file_mode(metadata: &Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o777
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        0
    }
}

fn file_digest(path: &Path) -> Result<Fingerprint> {
    let mut hash = Sha256::new();
    let mut reader = BufReader::new(File::open(path)?);
    let mut buffer = [0; 16 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().into())
}

fn hex(value: &Fingerprint) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn path_key(path: &Path) -> PathBuf {
    if cfg!(any(windows, target_os = "macos")) {
        PathBuf::from(path.to_string_lossy().to_uppercase())
    } else {
        path.to_owned()
    }
}

pub(crate) fn normalize_outputs(outputs: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    outputs
        .iter()
        .map(|path| {
            check_relative(path)
                .with_context(|| format!("invalid output path '{}'", path.display()))?;
            ensure!(
                !path
                    .as_os_str()
                    .as_encoded_bytes()
                    .iter()
                    .any(|byte| matches!(byte, b'*' | b'?' | b'[' | b']')),
                "expected a literal output path, got '{}'",
                path.display()
            );
            ensure!(!reserved(path), "reserved output path '{}'", path.display());
            Ok(path.components().collect())
        })
        .collect()
}

pub(crate) fn check_output(root: &Path, path: &Path) -> Result<Option<Metadata>> {
    ensure!(!reserved(path), "reserved output path '{}'", path.display());
    checked_metadata(root, path)
        .with_context(|| format!("unsafe cache output '{}'", path.display()))
}

fn reserved(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str();
        name.eq_ignore_ascii_case(".git") || name.eq_ignore_ascii_case(".lazel")
    }) || path.as_os_str().eq_ignore_ascii_case("lazel.toml")
}

fn check_relative(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path.components().all(|component| match component {
                Component::Normal(name) => valid_component(name),
                _ => false,
            }),
        "expected a safe relative path, got '{}'",
        path.display()
    );
    Ok(())
}

fn valid_component(name: &std::ffi::OsStr) -> bool {
    let valid = !name
        .as_encoded_bytes()
        .iter()
        .any(|byte| matches!(byte, b'\\' | b':' | 0));
    #[cfg(windows)]
    let valid = valid && valid_windows_component(name);
    valid
}

#[cfg(windows)]
fn valid_windows_component(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    if name.ends_with([' ', '.'])
        || name.contains(['<', '>', '"', '|', '?', '*'])
        || name.bytes().any(|byte| byte < 32)
    {
        return false;
    }
    let device = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    !matches!(
        device.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) && !device
        .strip_prefix("COM")
        .or_else(|| device.strip_prefix("LPT"))
        .is_some_and(|suffix| {
            matches!(
                suffix,
                "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
}

fn checked_metadata(root: &Path, path: &Path) -> Result<Option<Metadata>> {
    check_relative(path)?;
    let mut current = root.to_path_buf();
    let mut components = path.components().peekable();
    while let Some(component) = components.next() {
        current.push(component);
        let Some(metadata) = plain_metadata(&current)? else {
            return Ok(None);
        };
        #[cfg(windows)]
        {
            let canonical = match current.canonicalize() {
                Ok(path) => path,
                Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error.into()),
            };
            ensure!(
                path_key(&canonical) == path_key(&current),
                "unsupported path alias '{}'",
                current.display()
            );
        }
        if components.peek().is_none() {
            return Ok(Some(metadata));
        }
        ensure!(
            metadata.is_dir(),
            "'{}' is not a directory",
            current.display()
        );
    }
    Ok(None)
}

fn plain_metadata(path: &Path) -> Result<Option<Metadata>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("could not inspect '{}'", path.display()));
        }
    };
    ensure!(
        metadata.is_dir() || metadata.is_file(),
        "'{}' is not a regular file or directory",
        path.display()
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "'{}' is a reparse point",
            path.display()
        );
    }
    Ok(Some(metadata))
}

#[cfg(test)]
#[path = "../tests/unit/cache.rs"]
mod tests;
