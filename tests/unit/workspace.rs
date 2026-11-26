use super::*;

#[test]
fn diamond_plan_releases_each_task_after_all_dependencies() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("lazel.toml"),
        r#"
[projects.app]
dir = "."
[projects.app.tasks.base]
command = "echo base"
[projects.app.tasks.left]
command = "echo left"
depends_on = ["app:base", "app:base"]
[projects.app.tasks.right]
command = "echo right"
depends_on = ["app:base"]
[projects.app.tasks.build]
command = "echo build"
depends_on = ["app:left", "app:right"]
"#,
    )
    .unwrap();
    let workspace = Workspace::load(root.path()).unwrap();
    let mut plan = workspace
        .plan(&["app:build".into(), "app:base".into()], None)
        .unwrap();
    assert_eq!(
        plan.tasks.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        ["app:base", "app:left", "app:right", "app:build"]
    );
    assert_eq!(plan.ready.pop_first(), Some(0));
    assert!(plan.ready.is_empty());
    plan.complete(0);
    assert_eq!(plan.ready.pop_first(), Some(1));
    assert_eq!(plan.ready.pop_first(), Some(2));
    assert!(plan.ready.is_empty());
    plan.complete(2);
    assert!(plan.ready.is_empty());
    plan.complete(1);
    assert_eq!(plan.ready.pop_first(), Some(3));
    plan.complete(3);
    assert!(plan.ready.is_empty());
}
