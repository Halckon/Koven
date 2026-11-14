//! Real default project loader/build evidence. Does not execute the generated program.
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
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
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn ordinary_project_build_keeps_full_sources_for_nested_d10_shapes() {
    for body in [
        "val parentOld = replace(&a, \"parent\"); var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; outer -= 1",
        "var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; val parentOld = replace(&a, \"parent\"); outer -= 1",
    ] {
        for callee_first in [false, true] {
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
                "package app\nfun entry() { val p = 1; val c = 2; run(p, c) }",
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
            // Object/link proof only. Native behavior and independent D10 multi-source graph
            // admission remain separate obligations.
        }
    }
}
