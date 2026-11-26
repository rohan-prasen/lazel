use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const WORKSPACE: &str = r#"
[projects.app]
dir = "."
[projects.app.tasks.build]
command = "echo app"
depends_on = ["tools:generate"]
[projects.app.tasks.lint]
command = "echo lint"
[projects.app-next]
dir = "."
[projects.app-next.tasks.build]
command = "echo app-next"
[projects.tools]
dir = "."
[projects.tools.tasks.generate]
command = "echo tools"
"#;

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lazel"))
        .current_dir(root)
        .arg("run")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn selects_mixed_and_repeated_targets_in_order() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lazel.toml"), WORKSPACE).unwrap();
    let result = run(
        root.path(),
        &["lint", "build", "app:build", "lint", "--filter", "app*"],
    );
    assert!(result.status.success(), "{result:?}");
    assert_eq!(
        String::from_utf8_lossy(&result.stdout)
            .lines()
            .collect::<Vec<_>>(),
        ["lint", "tools", "app", "app-next"]
    );
}

#[test]
fn rejects_invalid_selection_before_launching_any_task() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("lazel.toml");
    fs::write(&config, WORKSPACE).unwrap();
    for (args, message) in [
        (&["build", "misspelled"][..], "no tasks match 'misspelled'"),
        (&["build", "app::lint"], "invalid task selector"),
        (&["build", "--filter", "APP*"], "no tasks match 'build'"),
        (&["build", "--filter", "["], "invalid project filter"),
        (
            &["build", "app-next:build", "--filter", "app"],
            "does not match the project filter",
        ),
    ] {
        let result = run(root.path(), args);
        assert_eq!(result.status.code(), Some(1), "{result:?}");
        assert!(result.stdout.is_empty(), "{result:?}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(message),
            "{result:?}"
        );
    }
    for limit in ["0", "-1", "2.5", "100000000000000000000"] {
        let result = run(root.path(), &["build", &format!("--concurrency={limit}")]);
        assert_eq!(result.status.code(), Some(2), "{result:?}");
        assert!(result.stdout.is_empty(), "{result:?}");
    }

    fs::write(
        &config,
        format!("{WORKSPACE}\n[projects.tools.tasks.broken]\ncommand = \"echo broken\"\ndepends_on = [\"missing:task\"]\n"),
    )
    .unwrap();
    let result = run(root.path(), &["build", "--filter", "app*"]);
    assert_eq!(result.status.code(), Some(1), "{result:?}");
    assert!(result.stdout.is_empty(), "{result:?}");
    assert!(String::from_utf8_lossy(&result.stderr).contains("unknown dependency"));
}

#[test]
fn parallel_plan_rejects_only_conflicting_output_writers_that_can_overlap() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("nested");
    fs::create_dir(&nested).unwrap();
    let config = root.path().join("lazel.toml");
    let workspace = r#"
[projects.outer]
dir = "."
[projects.outer.tasks.build]
command = "echo outer"
outputs = ["nested/out"]
[projects.outer.tasks.bridge]
command = "echo bridge"
depends_on = ["outer:build"]
[projects.inner]
dir = "nested"
[projects.inner.tasks.build]
command = "echo inner"
outputs = ["out/leaf"]
"#;
    let cache = "\ncache = true\ninputs = [\"source\"]\n";
    for workspace in [
        workspace.to_owned(),
        workspace.replace("out/leaf", "out"),
        format!("{workspace}{cache}"),
    ] {
        fs::write(&config, workspace).unwrap();
        let result = run(root.path(), &["build", "--concurrency", "2"]);
        assert_eq!(result.status.code(), Some(1), "{result:?}");
        assert!(result.stdout.is_empty(), "{result:?}");
        let stderr = String::from_utf8_lossy(&result.stderr);
        for text in ["overlapping parallel output", "outer:build", "inner:build"] {
            assert!(stderr.contains(text), "{stderr}");
        }
    }

    fs::write(&config, workspace).unwrap();
    assert!(run(root.path(), &["build"]).status.success());
    assert!(
        run(root.path(), &["inner:build", "--concurrency=2"])
            .status
            .success()
    );
    for fields in [
        "\ndepends_on = [\"outer:bridge\"]\n",
        "\ninteractive = true\n",
    ] {
        fs::write(&config, format!("{workspace}{fields}")).unwrap();
        let result = run(root.path(), &["build", "--concurrency=2"]);
        assert!(result.status.success(), "{result:?}");
    }

    fs::write(&config, format!("{workspace}{cache}\ninteractive = true")).unwrap();
    let result = run(root.path(), &["build"]);
    assert_eq!(result.status.code(), Some(1), "{result:?}");
    assert!(String::from_utf8_lossy(&result.stderr).contains("cannot enable caching"));

    fs::write(
        &config,
        format!(
            "{}{cache}",
            workspace.replace(
                "[projects.outer.tasks.bridge]",
                &format!("{cache}\n[projects.outer.tasks.bridge]")
            )
        ),
    )
    .unwrap();
    for limit in ["--concurrency=1", "--concurrency=2"] {
        let result = run(root.path(), &["inner:build", limit]);
        assert_eq!(result.status.code(), Some(1), "{result:?}");
        assert!(result.stdout.is_empty(), "{result:?}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("overlapping cache output"));
    }
}
