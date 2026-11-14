//! Default project evidence: compilation-only and separate bounded native execution.
use std::{
    fs,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Project(std::path::PathBuf);
impl Project {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-d10-project-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::create_dir_all(path.join("src/app")).unwrap();
        Self(path)
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("D10 failed project retained at {}", self.0.display());
        } else {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
#[test]
fn ordinary_project_build_keeps_full_sources_for_nested_d10_shapes() {
    for body in BODIES {
        for callee_first in [false, true] {
            let (_project, executable) = build_project(body, callee_first, 1, 2);
            assert!(fs::metadata(executable).unwrap().len() > 0);
        }
    }
}

const BODIES: [&str; 2] = [
    "val parentOld = replace(&a, \"parent\"); var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; outer -= 1",
    "var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; val parentOld = replace(&a, \"parent\"); outer -= 1",
];

fn build_project(
    body: &str,
    callee_first: bool,
    parent: u32,
    child: u32,
) -> (Project, std::path::PathBuf) {
    let project = Project::new();
    let manifest = project.0.join("project.toml");
    fs::write(&manifest, "schema = \"koven.project\"\nversion = 1\n[project]\nname = \"d10-project\"\nsource-roots = [\"src\"]\n").unwrap();
    let (callee_path, entry_path) = if callee_first {
        ("A.ko", "Z.ko")
    } else {
        ("Z.ko", "A.ko")
    };
    fs::write(project.0.join("src/app").join(callee_path), format!("package app\nfun run(count: Int, childCount: Int) {{ var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) {{ {body} }}; println(a); println(b) }}")).unwrap();
    fs::write(
        project.0.join("src/app").join(entry_path),
        format!("package app\nfun entry() {{ val p = {parent}; val c = {child}; run(p, c) }}"),
    )
    .unwrap();
    let executable = project.0.join("program");
    // Uses the normal project command and actual standard-source loader. No explicit
    // experimental mode, synthetic source, snapshot crop, or custom prelude.
    let output = Command::new(env!("CARGO_BIN_EXE_kovenc"))
        .arg("build")
        .arg("--project")
        .arg(&manifest)
        .args(["--entry", "app.entry", "-o"])
        .arg(&executable)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert!(fs::metadata(&executable).unwrap().len() > 0);
    (project, executable)
}

#[test]
fn ordinary_split_project_executes_parent_child_zero_and_multiple_rounds() {
    for (order, body) in BODIES.into_iter().enumerate() {
        for callee_first in [false, true] {
            for (parent, child) in [(0, 0), (0, 2), (2, 0), (2, 3)] {
                let (_project, executable) = build_project(body, callee_first, parent, child);
                let value = if parent == 0 {
                    "first"
                } else if order == 0 && child > 0 {
                    "next"
                } else {
                    "parent"
                };
                // Expected value derives from source loop semantics, not emitted SSA or output.
                let expected = format!("{value}\nkept\n");
                let mut process = Command::new(&executable)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    if process.try_wait().unwrap().is_some() {
                        break;
                    }
                    if Instant::now() >= deadline {
                        // Kill/reap only the child owned by this test. No retries or relaxed limit.
                        let _ = process.kill();
                        let output = process.wait_with_output().unwrap();
                        panic!(
                            "D10 native deadline: order={order} callee_first={callee_first} parent={parent} child={child}: {output:?}"
                        );
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                let output = process.wait_with_output().unwrap();
                assert!(output.status.success(), "{output:?}");
                assert_eq!(
                    output.stdout,
                    expected.as_bytes(),
                    "order={order} callee_first={callee_first} parent={parent} child={child}"
                );
                assert!(output.stderr.is_empty(), "{output:?}");
            }
        }
    }
}
