use std::env;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const LATEST: &str = "https://api.github.com/repos/rohan-prasen/lazel/releases/latest";
const DOWNLOADS: &str = "https://github.com/rohan-prasen/lazel/releases/download";
const HOME_VAR: &str = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
const BINARY: &str = if cfg!(windows) { "lazel.exe" } else { "lazel" };
const ASSET_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

pub fn execute() -> Result<i32> {
    let executable = env::current_exe().context("could not locate the running Lazel executable")?;
    let home = env::var_os(HOME_VAR).map(PathBuf::from);
    let destination = install_path(&executable, home.as_deref())?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into();

    update(&destination, env!("CARGO_PKG_VERSION"), |url| {
        let response = agent
            .get(url)
            .header("User-Agent", concat!("lazel/", env!("CARGO_PKG_VERSION")))
            .call()
            .with_context(|| format!("could not download {url}"))?;
        ensure!(
            response.status().is_success(),
            "unexpected HTTP status {} for {url}",
            response.status()
        );
        Ok(response.into_body())
    })?;
    Ok(0)
}

fn install_path(executable: &Path, home: Option<&Path>) -> Result<PathBuf> {
    let path = || -> Result<PathBuf> {
        let home = home
            .filter(|home| home.is_absolute())
            .with_context(|| format!("{HOME_VAR} must be an absolute path"))?;
        let expected = home.join(".local").join("bin").join(BINARY);
        check_regular_file(&expected)?;
        let expected = expected.canonicalize()?;
        ensure!(
            executable.canonicalize()? == expected,
            "expected to find the running executable at '{}'",
            expected.display()
        );
        Ok(expected)
    };
    path().with_context(|| {
        format!(
            "cannot self-update '{}'. Use your original installation method",
            executable.display()
        )
    })
}

fn check_regular_file(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("could not inspect '{}'", path.display()))?;
    ensure!(
        metadata.is_file(),
        "'{}' must be a regular file",
        path.display()
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        ensure!(
            metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
            "'{}' must not be a reparse point",
            path.display()
        );
    }
    Ok(())
}

fn parse_version(version: &str) -> Result<[u64; 3]> {
    let mut parts = version.split('.');
    let mut numbers = [0; 3];
    for number in &mut numbers {
        let part = parts.next().unwrap_or_default();
        ensure!(
            !part.is_empty()
                && (part == "0" || !part.starts_with('0'))
                && part.bytes().all(|byte| byte.is_ascii_digit()),
            "invalid Lazel version '{version}'"
        );
        *number = part
            .parse()
            .with_context(|| format!("invalid Lazel version '{version}'"))?;
    }
    ensure!(parts.next().is_none(), "invalid Lazel version '{version}'");
    Ok(numbers)
}

fn archive_name(os: &str, arch: &str) -> Result<String> {
    ensure!(
        matches!(arch, "x86_64" | "aarch64"),
        "no Lazel release for {os}/{arch}"
    );
    let suffix = match os {
        "macos" => "apple-darwin.tar.gz",
        "linux" => "unknown-linux-musl.tar.gz",
        "windows" => "pc-windows-msvc.zip",
        _ => bail!("no Lazel release for {os}/{arch}"),
    };
    Ok(format!("lazel-{arch}-{suffix}"))
}

fn update(
    destination: &Path,
    running: &str,
    mut fetch: impl FnMut(&str) -> Result<ureq::Body>,
) -> Result<()> {
    let release: Release = fetch(LATEST)?
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_json()
        .context("could not read the latest Lazel release")?;
    let latest = release
        .tag_name
        .strip_prefix('v')
        .context("expected a Lazel release tag starting with 'v'")?;
    let latest_version = parse_version(latest)?;
    if latest_version <= parse_version(running)? {
        println!("Lazel {running} is up to date (latest release: {latest}).");
        return Ok(());
    }

    let parent = destination
        .parent()
        .context("missing installation directory")?;
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(parent.join(".lazel-update.lock"))
        .context("could not open the Lazel update lock")?;
    lock.try_lock()
        .context("could not lock the installation for update")?;

    let archive = archive_name(env::consts::OS, env::consts::ARCH)?;
    let url = format!("{DOWNLOADS}/{}/{archive}", release.tag_name);
    eprintln!("lazel: downloading {archive} for version {latest}");
    let checksum = fetch(&format!("{url}.sha256"))?
        .with_config()
        .limit(4096)
        .read_to_string()
        .context("could not read the release checksum")?;
    let expected = checksum.split_ascii_whitespace().next().unwrap_or_default();
    ensure!(
        expected.len() == 64 && expected.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid Lazel release checksum"
    );
    let bytes = fetch(&url)?
        .with_config()
        .limit(ASSET_LIMIT)
        .read_to_vec()
        .context("could not read the release archive")?;
    let actual = sha256_hex(&bytes);
    ensure!(
        actual.eq_ignore_ascii_case(expected),
        "Lazel download failed SHA-256 verification"
    );

    let stage = tempfile::Builder::new()
        .prefix(".lazel-update.")
        .tempdir_in(parent)
        .context("could not create an update staging directory")?;
    let staged = stage.path().join(BINARY);
    extract(&bytes, &staged).context("could not extract the Lazel executable")?;
    let staged_version = binary_version(&staged)?;
    ensure!(
        staged_version == latest,
        "expected Lazel {latest} in the release archive, found {staged_version}"
    );

    check_regular_file(destination)?;
    let installed = binary_version(destination)?;
    if latest_version <= parse_version(&installed)? {
        println!("Lazel {installed} is up to date (latest release: {latest}).");
        return Ok(());
    }
    replace_executable(destination, &staged)
        .with_context(|| format!("could not replace '{}'", destination.display()))?;
    println!("Updated Lazel from {installed} to {latest}.");
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn binary_version(path: &Path) -> Result<String> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .with_context(|| format!("could not run '{}' to check its version", path.display()))?;
    ensure!(
        output.status.success(),
        "'{} --version' failed ({})",
        path.display(),
        output.status
    );
    let stdout = std::str::from_utf8(&output.stdout).context("invalid Lazel version output")?;
    let version = stdout
        .trim()
        .strip_prefix("lazel ")
        .with_context(|| format!("unexpected version output from '{}'", path.display()))?;
    parse_version(version)?;
    Ok(version.to_owned())
}

fn copy_binary(reader: impl Read, destination: &Path) -> Result<()> {
    let mut file = File::create_new(destination)?;
    let size = io::copy(&mut reader.take(ASSET_LIMIT + 1), &mut file)?;
    ensure!(size > 0 && size <= ASSET_LIMIT, "invalid executable size");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o755))?;
    }
    file.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn extract(bytes: &[u8], destination: &Path) -> Result<()> {
    let decoded = flate2::read::GzDecoder::new(bytes).take(ASSET_LIMIT + 1024 * 1024);
    let mut archive = tar::Archive::new(decoded);
    let mut found = false;
    for entry in archive.entries()? {
        let entry = entry?;
        if entry.path_bytes() == BINARY.as_bytes() {
            ensure!(
                !found && entry.header().entry_type().is_file(),
                "archive must contain one regular {BINARY} entry"
            );
            copy_binary(entry, destination)?;
            found = true;
        }
    }
    ensure!(found, "archive is missing {BINARY}");
    let mut decoded = archive.into_inner();
    io::copy(&mut decoded, &mut io::sink())?;
    ensure!(decoded.limit() > 0, "uncompressed archive is too large");
    Ok(())
}

#[cfg(windows)]
fn extract(bytes: &[u8], destination: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(io::Cursor::new(bytes))?;
    ensure!(
        !archive.has_duplicate_names(),
        "archive contains duplicate file names"
    );
    let index = archive
        .index_for_name_raw(BINARY.as_bytes())
        .with_context(|| format!("archive is missing {BINARY}"))?;
    let entry = archive.by_index(index)?;
    ensure!(
        entry.is_file(),
        "archive must contain a regular {BINARY} entry"
    );
    copy_binary(entry, destination)
}

#[cfg(unix)]
fn replace_executable(destination: &Path, staged: &Path) -> Result<()> {
    fs::rename(staged, destination)?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/update.rs"]
mod tests;

#[cfg(windows)]
const BACKUP: &str = ".lazel-update.previous.exe";

#[cfg(windows)]
fn replace_executable(destination: &Path, staged: &Path) -> Result<()> {
    let backup = destination.with_file_name(BACKUP);
    match backup.symlink_metadata() {
        Ok(_) => {
            check_regular_file(&backup)?;
            fs::remove_file(&backup).context("could not remove the previous Lazel backup")?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("could not inspect the previous Lazel backup"),
    }
    // Windows can rename a running executable even though it cannot overwrite it in place.
    fs::rename(destination, &backup).context("could not move the running executable aside")?;
    if let Err(error) = fs::rename(staged, destination) {
        if let Err(restore) = fs::rename(&backup, destination) {
            bail!(
                "could not install the update ({error}) or restore the previous executable \
                 ({restore}). The previous executable is saved at '{}'",
                backup.display()
            );
        }
        return Err(error)
            .context("could not install the update; restored the previous executable");
    }
    Ok(())
}
