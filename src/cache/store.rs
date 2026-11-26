use std::fs::{self, File, Metadata, TryLockError};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{Fingerprint, check_output, checked_metadata, field, file_digest, file_mode, hex};
use crate::workspace::Task;

const STATE: &str = ".lazel";
const RESTORING: &str = ".lazel/restoring";
const MANIFEST: &str = "manifest.toml";
const VERSION: u32 = 1;
const MANIFEST_LIMIT: usize = 8 * 1024 * 1024;

pub(crate) struct LocalCache<'a> {
    root: &'a Path,
    _lock: File,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    key: String,
    project: PathBuf,
    roots: Vec<PathBuf>,
    entries: Vec<Entry>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: PathBuf,
    file: Option<FileEntry>,
}

#[derive(Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct FileEntry {
    size: u64,
    mode: u32,
    sha256: String,
}

impl<'a> LocalCache<'a> {
    pub fn lock(root: &'a Path, interrupted: Option<fn() -> bool>) -> Result<Self> {
        let state = Path::new(STATE);
        if checked_metadata(root, state)?.is_none() {
            match fs::create_dir(root.join(state)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error).context("could not create local cache state"),
            }
        }
        ensure!(
            checked_metadata(root, state)?.is_some_and(|meta| meta.is_dir()),
            "cache state is not a directory"
        );
        let lock_path = state.join("cache.lock");
        ensure!(
            !checked_metadata(root, &lock_path)?.is_some_and(|meta| meta.is_dir()),
            "cache lock is not a file"
        );
        let lock = File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join(lock_path))?;
        if let Some(interrupted) = interrupted {
            let mut waiting = false;
            loop {
                if interrupted() {
                    return Err(io::Error::from(io::ErrorKind::Interrupted).into());
                }
                match lock.try_lock() {
                    Ok(()) => break,
                    Err(TryLockError::WouldBlock) => {
                        if !waiting {
                            eprintln!("lazel: waiting for the local cache workspace lock");
                            waiting = true;
                        }
                        thread::sleep(Duration::from_millis(25));
                    }
                    Err(TryLockError::Error(error)) => {
                        return Err(error).context("could not lock local cache workspace");
                    }
                }
            }
        } else {
            lock.lock()
                .context("could not lock local cache workspace")?;
        }
        ensure!(
            checked_metadata(root, Path::new(RESTORING))?.is_none(),
            "interrupted cache restore; inspect '{}' for the manifest and previous outputs before running tasks",
            root.join(RESTORING).display()
        );
        Ok(Self { root, _lock: lock })
    }

    pub fn store(
        &self,
        task: &Task,
        outputs: &[PathBuf],
        key: &Fingerprint,
    ) -> Result<Fingerprint> {
        let cache_root = Path::new(STATE).join("cache");
        let absolute_cache = self.root.join(&cache_root);
        if checked_metadata(self.root, &cache_root)?.is_none() {
            fs::create_dir(&absolute_cache)?;
        }
        let stage = tempfile::tempdir_in(&absolute_cache)?;
        let keys = hex(key);
        let manifest = Manifest {
            version: VERSION,
            key: keys.clone(),
            project: task.project.clone(),
            roots: outputs.to_vec(),
            entries: copy_outputs(
                self.root,
                &task.project,
                outputs,
                &stage.path().join("payload"),
            )?,
        };
        let encoded = toml::to_string(&manifest)?;
        ensure!(
            encoded.len() <= MANIFEST_LIMIT,
            "cache manifest exceeds size limit"
        );
        fs::write(stage.path().join(MANIFEST), &encoded)?;

        let target = absolute_cache.join(keys);
        if target.symlink_metadata().is_ok() {
            let discard = tempfile::tempdir_in(&absolute_cache)?;
            fs::rename(&target, discard.path().join("entry"))?;
        }
        fs::rename(stage.path(), &target)?;
        Ok(result_fingerprint(key, &encoded))
    }

    pub fn restore(
        &self,
        task: &Task,
        outputs: &[PathBuf],
        key: &Fingerprint,
    ) -> Result<Option<Fingerprint>> {
        let relative = Path::new(STATE).join("cache").join(hex(key));
        let prepared = self.prepare(&relative, &task.project, outputs, key);
        let Some((stage, fingerprint)) = (match prepared {
            Ok(result) => result,
            Err(error) => {
                eprintln!("lazel: ignoring invalid cache entry: {error:#}");
                None
            }
        }) else {
            return Ok(None);
        };

        if !outputs.is_empty() {
            let pending = self.root.join(RESTORING);
            fs::create_dir(stage.path().join("backup"))?;
            fs::rename(stage.path(), &pending).context("could not begin cache restore")?;
            self.replace_outputs(&pending, &task.project, outputs)?;
        }
        Ok(Some(fingerprint))
    }

    fn prepare(
        &self,
        entry: &Path,
        project: &Path,
        outputs: &[PathBuf],
        key: &Fingerprint,
    ) -> Result<Option<(tempfile::TempDir, Fingerprint)>> {
        if checked_metadata(self.root, entry)?.is_none() {
            return Ok(None);
        }
        let relative = entry.join(MANIFEST);
        let info = checked_metadata(self.root, &relative)?
            .filter(Metadata::is_file)
            .context("missing cache manifest")?;
        ensure!(
            info.len() <= MANIFEST_LIMIT as u64,
            "invalid cache manifest size"
        );
        let mut encoded = String::new();
        File::open(self.root.join(&relative))?
            .take((MANIFEST_LIMIT + 1) as u64)
            .read_to_string(&mut encoded)?;
        ensure!(
            encoded.len() <= MANIFEST_LIMIT,
            "invalid cache manifest size"
        );
        let manifest: Manifest =
            toml::from_str(&encoded).map_err(|_| anyhow::anyhow!("invalid cache manifest"))?;
        ensure!(
            manifest.version == VERSION
                && manifest.key == hex(key)
                && manifest.project == project
                && manifest.roots == outputs,
            "cache manifest does not match task"
        );
        let stage = tempfile::tempdir_in(self.root.join(STATE))?;
        let verified = copy_outputs(
            self.root,
            &entry.join("payload"),
            outputs,
            &stage.path().join("payload"),
        )?;
        ensure!(
            manifest.entries == verified,
            "cache artifact contents do not match manifest"
        );
        fs::write(stage.path().join(MANIFEST), &encoded)?;
        Ok(Some((stage, result_fingerprint(key, &encoded))))
    }

    fn replace_outputs(&self, stage: &Path, project: &Path, outputs: &[PathBuf]) -> Result<()> {
        let mut changes = Vec::new();
        let mut created_parents = Vec::new();
        let result = (|| -> Result<()> {
            for (index, output) in outputs.iter().enumerate() {
                let relative = project.join(output);
                let previous = check_output(self.root, &relative)?;
                let target = self.root.join(&relative);
                let backup = stage.join("backup").join(index.to_string());
                let new = stage.join("payload").join(output);
                if let Some(parent) = relative.parent() {
                    let mut ancestor = PathBuf::new();
                    for component in parent.components() {
                        ancestor.push(component);
                        if checked_metadata(self.root, &ancestor)?.is_none() {
                            let path = self.root.join(&ancestor);
                            fs::create_dir(&path)?;
                            created_parents.push(path);
                        }
                    }
                }
                let mut change = Replacement {
                    target,
                    backup,
                    new,
                    backed_up: false,
                    installed: false,
                };
                if previous.is_some() {
                    fs::rename(&change.target, &change.backup)?;
                    change.backed_up = true;
                }
                changes.push(change);
                let change = &mut changes[index];
                fs::rename(&change.new, &change.target)?;
                change.installed = true;
            }
            Ok(())
        })();
        if let Err(error) = result {
            if let Err(restore) = roll_back(changes, created_parents) {
                bail!(
                    "cache restore failed ({error:#}) and rollback failed ({restore:#}); previous outputs may be under '{}'",
                    stage.display()
                );
            }
            fs::remove_dir_all(stage).context("could not clear rolled-back cache restore")?;
            return Err(error)
                .context("could not restore cached outputs; previous outputs restored");
        }
        fs::remove_dir_all(stage).context("could not clear completed cache restore")?;
        Ok(())
    }
}

struct Replacement {
    target: PathBuf,
    backup: PathBuf,
    new: PathBuf,
    backed_up: bool,
    installed: bool,
}

fn roll_back(changes: Vec<Replacement>, created_parents: Vec<PathBuf>) -> Result<()> {
    let mut result = Ok(());
    for change in changes.into_iter().rev() {
        let restored = (|| -> Result<()> {
            if change.installed {
                fs::rename(change.target.as_path(), change.new)?;
            }
            if change.backed_up {
                fs::rename(change.backup, change.target)?;
            }
            Ok(())
        })();
        result = result.and(restored);
    }
    for parent in created_parents.into_iter().rev() {
        let removed = fs::remove_dir(parent).context("could not remove a created output directory");
        result = result.and(removed);
    }
    result
}

fn copy_outputs(
    root: &Path,
    source: &Path,
    outputs: &[PathBuf],
    destination: &Path,
) -> Result<Vec<Entry>> {
    fs::create_dir(destination)?;
    let mut entries = Vec::new();
    let mut pending = outputs.to_vec();
    while let Some(path) = pending.pop() {
        ensure!(path.to_str().is_some(), "cache output path is not UTF-8");
        let relative = source.join(&path);
        let metadata = checked_metadata(root, &relative)?
            .with_context(|| format!("missing declared output '{}'", path.display()))?;
        let source_path = root.join(&relative);
        let target = destination.join(&path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = if metadata.is_dir() {
            fs::create_dir(&target)?;
            for child in source_path.read_dir()? {
                pending.push(path.join(child?.file_name()));
            }
            None
        } else {
            let size = fs::copy(&source_path, &target)?;
            let mode = file_mode(&metadata);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&target, fs::Permissions::from_mode(mode))?;
            }
            Some(FileEntry {
                size,
                mode,
                sha256: hex(&file_digest(&target)?),
            })
        };
        entries.push(Entry { path, file });
    }
    entries.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

fn result_fingerprint(key: &Fingerprint, manifest: &str) -> Fingerprint {
    let mut hash = Sha256::new();
    field(&mut hash, key);
    field(&mut hash, manifest.as_bytes());
    hash.finalize().into()
}

#[cfg(test)]
#[path = "../../tests/unit/cache_store.rs"]
mod tests;
