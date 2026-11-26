use super::*;

#[test]
fn failed_install_rolls_back_all_replaced_outputs() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let cache = LocalCache::lock(&root, None).unwrap();
    let stage = root.join(RESTORING);
    fs::create_dir_all(stage.join("payload")).unwrap();
    fs::create_dir_all(stage.join("backup")).unwrap();
    fs::write(stage.join("payload/first.txt"), "new first").unwrap();
    fs::write(root.join("first.txt"), "old first").unwrap();
    fs::write(root.join("second.txt"), "old second").unwrap();

    let error = cache
        .replace_outputs(
            &stage,
            Path::new(""),
            &["first.txt".into(), "second.txt".into()],
        )
        .unwrap_err();
    assert!(error.to_string().contains("previous outputs restored"));
    assert_eq!(
        fs::read_to_string(root.join("first.txt")).unwrap(),
        "old first"
    );
    assert_eq!(
        fs::read_to_string(root.join("second.txt")).unwrap(),
        "old second"
    );
    assert!(!stage.exists());
}

#[test]
fn a_pending_restore_blocks_new_work_until_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::create_dir_all(root.join(RESTORING)).unwrap();
    let error = LocalCache::lock(&root, None).err().unwrap();
    assert!(error.to_string().contains("interrupted cache restore"));
    assert!(root.join(RESTORING).is_dir());
}
