use super::*;

#[test]
fn fingerprint_uses_declared_inputs_and_distinguishes_missing_environment() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let task = Task {
        dir: root.clone(),
        project: PathBuf::new(),
        command: "echo test".into(),
        interactive: false,
        depends_on: vec![],
        outputs: vec!["dist".into()],
        cache: None,
    };
    let config = CacheConfig::new(true, vec!["**/*".into()], vec!["TOOLCHAIN".into()])
        .unwrap()
        .unwrap();
    let mut environment = BTreeMap::from([("TOOLCHAIN".into(), None)]);
    let key = |env: &TaskEnvironment, deps: &[Fingerprint]| {
        fingerprint(&root, "app:build", &task, &config, env, deps).unwrap()
    };
    fs::write(root.join("source.txt"), "first").unwrap();
    let original = key(&environment, &[]);
    assert_ne!(original, key(&environment, &[[1; 32]]));
    environment.insert("TOOLCHAIN".into(), Some("".into()));
    assert_ne!(original, key(&environment, &[]));
    environment.insert("TOOLCHAIN".into(), Some("compiler-v2".into()));
    assert_ne!(original, key(&environment, &[]));
    environment.insert("TOOLCHAIN".into(), None);
    for excluded in ["dist", ".lazel", ".git"] {
        fs::create_dir(root.join(excluded)).unwrap();
        fs::write(root.join(excluded).join("secret.txt"), "contents").unwrap();
    }
    fs::write(root.join("lazel.toml"), "configuration").unwrap();
    assert_eq!(original, key(&environment, &[]));
    fs::write(root.join("Cargo.lock"), "new dependency").unwrap();
    assert_ne!(original, key(&environment, &[]));
    fs::remove_file(root.join("Cargo.lock")).unwrap();
    assert_eq!(original, key(&environment, &[]));
    fs::write(root.join("source.txt"), "other").unwrap();
    assert_ne!(original, key(&environment, &[]));
    let environment = config.environment();
    assert!(environment.contains_key("PATH"));
    #[cfg(windows)]
    assert!(environment.contains_key("SYSTEMROOT") && environment.contains_key("PATHEXT"));
}
