use super::*;
use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;

fn release(tag: &str) -> ureq::Body {
    ureq::Body::builder().data(format!(r#"{{"tag_name":"{tag}"}}"#))
}

fn downloads<'a>(
    tag: &'a str,
    bytes: &'a [u8],
    checksum: &'a str,
) -> impl FnMut(&str) -> Result<ureq::Body> + 'a {
    let archive = archive_name(env::consts::OS, env::consts::ARCH).unwrap();
    let archive_url = format!("{DOWNLOADS}/{tag}/{archive}");
    let checksum_url = format!("{archive_url}.sha256");
    move |url| {
        if url == LATEST {
            Ok(release(tag))
        } else if url == checksum_url {
            Ok(ureq::Body::builder().data(format!("{checksum}  {archive}\n")))
        } else {
            assert_eq!(url, archive_url);
            Ok(ureq::Body::builder().data(bytes.to_vec()))
        }
    }
}

#[test]
fn selects_each_published_archive() {
    for (os, arch, expected) in [
        ("macos", "aarch64", "lazel-aarch64-apple-darwin.tar.gz"),
        ("macos", "x86_64", "lazel-x86_64-apple-darwin.tar.gz"),
        (
            "linux",
            "aarch64",
            "lazel-aarch64-unknown-linux-musl.tar.gz",
        ),
        ("linux", "x86_64", "lazel-x86_64-unknown-linux-musl.tar.gz"),
        ("windows", "aarch64", "lazel-aarch64-pc-windows-msvc.zip"),
        ("windows", "x86_64", "lazel-x86_64-pc-windows-msvc.zip"),
    ] {
        assert_eq!(archive_name(os, arch).unwrap(), expected);
    }
    assert!(archive_name("linux", "arm").is_err());
}

#[test]
fn equal_and_older_releases_are_no_ops() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join(BINARY);
    fs::write(&destination, b"current").unwrap();
    for tag in ["v0.1.10", "v0.1.9"] {
        update(&destination, "0.1.10", |url| {
            assert_eq!(url, LATEST);
            Ok(release(tag))
        })
        .unwrap();
    }
    assert_eq!(fs::read(&destination).unwrap(), b"current");
    assert!(!directory.path().join(".lazel-update.lock").exists());
}

#[test]
fn rejects_malformed_release_tags() {
    for tag in ["0.1.5", "v0.1", "v0.1.05", "v0.1.5-rc.1", "v0.1.5/other"] {
        assert!(
            update(Path::new(BINARY), "0.1.4", |url| {
                assert_eq!(url, LATEST);
                Ok(release(tag))
            })
            .is_err(),
            "{tag}"
        );
    }
}

#[test]
fn download_and_archive_errors_preserve_the_installation() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join(BINARY);
    fs::write(&destination, b"current").unwrap();
    let missing = update(&destination, "0.1.4", |url| {
        if url == LATEST {
            Ok(release("v0.1.5"))
        } else {
            bail!("asset is unavailable")
        }
    });
    assert!(
        missing
            .unwrap_err()
            .to_string()
            .contains("asset is unavailable")
    );
    assert_eq!(fs::read(&destination).unwrap(), b"current");

    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let wrong_entry = package("nested/lazel", b"content");
    for (bytes, checksum, error) in [
        (
            &b"bad archive"[..],
            "bad".to_owned(),
            "invalid Lazel release checksum",
        ),
        (
            &b"bad archive"[..],
            "0".repeat(64),
            "failed SHA-256 verification",
        ),
        (
            &b"bad archive"[..],
            sha256_hex(b"bad archive"),
            "could not extract",
        ),
        (
            &wrong_entry[..],
            sha256_hex(&wrong_entry),
            "could not extract",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join(BINARY);
        fs::write(&destination, b"current").unwrap();
        let result = update(&destination, "0.1.4", downloads("v0.1.5", bytes, &checksum));
        let actual = result.unwrap_err();
        assert!(actual.to_string().contains(error), "{actual:#}");
        assert_eq!(fs::read(&destination).unwrap(), b"current");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }
}

#[cfg(windows)]
#[test]
fn rejects_duplicate_zip_entries() {
    const OTHER_NAME: &[u8] = b"other.exe";
    let mut archive = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for name in [BINARY, std::str::from_utf8(OTHER_NAME).unwrap()] {
        archive.start_file(name, options).unwrap();
        archive.write_all(b"fixture").unwrap();
    }
    let mut bytes = archive.finish().unwrap().into_inner();
    let offsets: Vec<_> = bytes
        .windows(OTHER_NAME.len())
        .enumerate()
        .filter_map(|(offset, name)| (name == OTHER_NAME).then_some(offset))
        .collect();
    assert_eq!(offsets.len(), 2);
    for offset in offsets {
        bytes[offset..offset + OTHER_NAME.len()].copy_from_slice(BINARY.as_bytes());
    }
    let directory = tempfile::tempdir().unwrap();
    let error = extract(&bytes, &directory.path().join(BINARY)).unwrap_err();
    assert!(
        error.to_string().contains("duplicate file names"),
        "{error:#}"
    );
}

#[test]
fn another_updater_holds_the_installation_lock() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join(BINARY);
    let lock = File::create(directory.path().join(".lazel-update.lock")).unwrap();
    lock.try_lock().unwrap();
    let result = update(&destination, "0.1.4", |url| {
        assert_eq!(url, LATEST);
        Ok(release("v0.1.5"))
    });
    assert!(result.unwrap_err().to_string().contains("could not lock"));
}

#[cfg(unix)]
#[test]
fn refuses_a_symlink_at_the_official_install_path() {
    let home = tempfile::tempdir().unwrap();
    let parent = home.path().join(".local/bin");
    fs::create_dir_all(&parent).unwrap();
    let elsewhere = home.path().join(BINARY);
    fs::write(&elsewhere, b"other").unwrap();
    std::os::unix::fs::symlink(&elsewhere, parent.join(BINARY)).unwrap();
    assert!(install_path(&elsewhere, Some(home.path())).is_err());
}

#[test]
fn replaces_a_running_executable_and_rejects_a_downgrade() {
    let home = tempfile::Builder::new()
        .prefix("Lazel café ")
        .tempdir()
        .unwrap();
    let parent = home.path().join(".local").join("bin");
    fs::create_dir_all(&parent).unwrap();
    let destination = parent.join(BINARY);
    let candidate = home.path().join(format!("candidate-{BINARY}"));
    build_fixture("0.1.4", &destination);
    build_fixture("0.1.10", &candidate);
    let destination = install_path(&destination, Some(home.path())).unwrap();
    let old_bytes = fs::read(&destination).unwrap();
    let archive = package(BINARY, &fs::read(&candidate).unwrap());
    let checksum = sha256_hex(&archive);

    let mismatch = update(
        &destination,
        "0.1.4",
        downloads("v0.1.9", &archive, &checksum),
    );
    let error = mismatch.unwrap_err();
    assert!(error.to_string().contains("found 0.1.10"), "{error:#}");
    assert_eq!(fs::read(&destination).unwrap(), old_bytes);

    let mut running = Command::new(&destination)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    BufReader::new(running.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready.trim(), "ready");

    #[cfg(windows)]
    {
        let error = replace_executable(&destination, &home.path().join("missing.exe")).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("restored the previous executable")
        );
        assert_eq!(fs::read(&destination).unwrap(), old_bytes);
        assert!(!parent.join(BACKUP).exists());
    }

    let result = update(
        &destination,
        "0.1.4",
        downloads("v0.1.10", &archive, &checksum),
    );
    assert!(running.wait().unwrap().success());
    result.unwrap();
    assert_eq!(binary_version(&destination).unwrap(), "0.1.10");
    #[cfg(windows)]
    assert_eq!(fs::read(parent.join(BACKUP)).unwrap(), old_bytes);

    let older = package(BINARY, &old_bytes);
    update(
        &destination,
        "0.1.3",
        downloads("v0.1.4", &older, &sha256_hex(&older)),
    )
    .unwrap();
    assert_eq!(binary_version(&destination).unwrap(), "0.1.10");
}

fn build_fixture(version: &str, destination: &Path) {
    let mut compiler = Command::new("rustc")
        .args([
            "--edition=2024",
            "--crate-name",
            "lazel_update_fixture",
            "-",
            "-o",
        ])
        .arg(destination)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    write!(
        compiler.stdin.take().unwrap(),
        r#"fn main() {{
            if std::env::args().nth(1).as_deref() == Some("--version") {{
                println!("lazel {version}");
            }} else {{
                println!("ready");
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                std::io::stdin().read_line(&mut String::new()).unwrap();
            }}
        }}"#
    )
    .unwrap();
    assert!(compiler.wait().unwrap().success());
}

#[cfg(unix)]
fn package(name: &str, bytes: &[u8]) -> Vec<u8> {
    let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut archive = tar::Builder::new(gzip);
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o755);
    archive.append_data(&mut header, name, bytes).unwrap();
    archive.into_inner().unwrap().finish().unwrap()
}

#[cfg(windows)]
fn package(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut archive = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    archive.start_file(name, options).unwrap();
    archive.write_all(bytes).unwrap();
    archive.finish().unwrap().into_inner()
}
