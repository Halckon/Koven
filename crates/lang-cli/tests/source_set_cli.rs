//! Public project input must be complete, deterministic and independent of language analysis.
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-source-set-{}-{} 项目 #%",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn manifest(&self, roots: &[&str]) {
        for root in roots {
            fs::create_dir_all(self.0.join(root)).unwrap();
        }
        fs::write(self.0.join("project.toml"), format!(
            "schema = \"koven.project\"\nversion = 1\n[project]\nname = \"example\"\nsource-roots = {roots:?}\n"
        )).unwrap();
    }
    fn run(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_kovenc"))
            .current_dir(&self.0)
            .args(["source-set", "--project", "project.toml"])
            .output()
            .unwrap()
    }
    fn snapshot(&self) -> Value {
        let result = self.run();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty());
        serde_json::from_slice(&result.stdout).unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn exports_complete_sorted_base_without_parsing_source() {
    let p = Project::new();
    p.manifest(&["z", "a"]);
    fs::write(p.0.join("z/Z.ko"), "not valid Koven {\r\n😀").unwrap();
    fs::write(p.0.join("a/A.ko"), "fun a(): Int = 1\n").unwrap();
    let snapshot = p.snapshot();
    assert_eq!(snapshot.as_object().unwrap().len(), 4);
    assert_eq!(snapshot["schema"], "koven.lsp.source-set");
    assert_eq!(snapshot["version"], 1);
    assert_eq!(snapshot["roots"], json!(["a", "z"]));
    assert_eq!(snapshot["sources"][0]["root"], "a");
    assert_eq!(snapshot["sources"][0]["logicalPath"], "A.ko");
    assert_eq!(snapshot["sources"][1]["text"], "not valid Koven {\r\n😀");
    for source in snapshot["sources"].as_array().unwrap() {
        assert_eq!(source.as_object().unwrap().len(), 4);
        let uri = source["uri"].as_str().unwrap();
        assert!(uri.starts_with("file:///"));
        assert!(uri.contains("%20%E9%A1%B9%E7%9B%AE%20%23%25/"), "{uri}");
        assert!(!uri.contains([' ', '#', '项']));
    }
    assert_eq!(p.run().stdout, p.run().stdout);
    p.manifest(&["a", "z"]);
    assert_eq!(
        snapshot,
        p.snapshot(),
        "Manifest enumeration order must not become identity"
    );
}

#[test]
fn empty_unit_is_valid_and_each_export_reads_current_disk() {
    let p = Project::new();
    p.manifest(&["src"]);
    assert_eq!(p.snapshot()["sources"], json!([]));
    fs::write(p.0.join("src/Before.ko"), "old").unwrap();
    assert_eq!(p.snapshot()["sources"][0]["text"], "old");
    fs::rename(p.0.join("src/Before.ko"), p.0.join("src/After.ko")).unwrap();
    fs::write(p.0.join("src/After.ko"), "new").unwrap();
    let updated = p.snapshot();
    assert_eq!(updated["sources"].as_array().unwrap().len(), 1);
    assert_eq!(updated["sources"][0]["logicalPath"], "After.ko");
    assert_eq!(updated["sources"][0]["text"], "new");
    fs::remove_file(p.0.join("src/After.ko")).unwrap();
    assert_eq!(p.snapshot()["sources"], json!([]));
}

#[test]
fn loader_failures_never_publish_a_partial_snapshot() {
    let p = Project::new();
    p.manifest(&["src"]);
    fs::write(p.0.join("src/Good.ko"), "fun good(): Unit {}\n").unwrap();
    fs::write(p.0.join("src/Bad.ko"), [0xff]).unwrap();
    let output = p.run();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    fs::remove_file(p.0.join("src/Bad.ko")).unwrap();
    fs::write(p.0.join("project.toml"), "bad = true").unwrap();
    let output = p.run();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

#[test]
fn usage_is_explicit_and_json_diagnostic_mode_does_not_wrap_the_export() {
    for args in [
        vec!["source-set"],
        vec!["source-set", "project.toml"],
        vec!["source-set", "--project", "project.toml", "extra"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_kovenc"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("source-set --project"));
    }
    let p = Project::new();
    p.manifest(&["src"]);
    let output = Command::new(env!("CARGO_BIN_EXE_kovenc"))
        .current_dir(&p.0)
        .args([
            "--message-format=json",
            "source-set",
            "--project",
            "project.toml",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, p.run().stdout);
}

#[test]
fn paths_outside_source_set_v1_fail_before_publishing() {
    for relative in ["foo-bar/Main.ko", r"A\B.ko"] {
        let p = Project::new();
        p.manifest(&["src"]);
        let source = p.0.join("src").join(relative);
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(source, "fun main(): Unit {}\n").unwrap();
        let output = p.run();
        assert_eq!(output.status.code(), Some(1), "{relative}");
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}
