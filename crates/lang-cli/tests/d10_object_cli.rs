//! Explicit object-only experimental command. Never links or executes generated code.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "koven-d10-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn entries(&self) -> Vec<PathBuf> {
        let mut v = fs::read_dir(&self.0)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        v.sort();
        v
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn command(source: &Path, output: &Path, entry: &str, json: bool) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_kovenc"));
    if json {
        c.arg("--message-format=json");
    }
    c.arg("experimental-d10-object")
        .arg(source)
        .args(["--entry", entry, "-o"])
        .arg(output)
        .output()
        .unwrap()
}
#[cfg(not(feature = "d10-catalog-experiment"))]
#[test]
fn default_cli_does_not_select_experimental_d10() {
    let d = Directory::new();
    let source = d.0.join("source.ko");
    fs::write(&source, "fun entry() {}").unwrap();
    let before = d.entries();
    let out = command(&source, &d.0.join("object.o"), "entry", false);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert_eq!(d.entries(), before);
    let help = Command::new(env!("CARGO_BIN_EXE_kovenc"))
        .arg("help")
        .output()
        .unwrap();
    assert!(
        !String::from_utf8(help.stdout)
            .unwrap()
            .contains("experimental-d10-object")
    );
}
#[cfg(feature = "d10-catalog-experiment")]
mod enabled {
    use super::*;
    const BEFORE: &str = r#"fun run(count: Int, childCount: Int) {
var a = "first"; var b = "kept"; var outer = count;
while(outer > 0) { val parentOld = replace(&a, "parent"); var inner = childCount;
while(inner > 0) { val old = replace(&a, "next"); inner -= 1 }; outer -= 1 };
println(a); println(b) }
fun entry() { val p = 1; val c = 2; run(p, c) }"#;
    #[test]
    fn both_orders_emit_real_objects_only_for_the_exact_source_entry() {
        let after = BEFORE
            .replace("val parentOld = replace(&a, \"parent\"); ", "")
            .replace(
                "}; outer -= 1",
                "}; val parentOld = replace(&a, \"parent\"); outer -= 1",
            );
        for text in [BEFORE, after.as_str()] {
            let (callee, wrapper) = text.split_once("fun entry()").unwrap();
            let reversed = format!("fun entry(){wrapper}\n{callee}");
            for text in [text, reversed.as_str()] {
                let d = Directory::new();
                let source = d.0.join("source.ko");
                fs::write(&source, text).unwrap();
                let object = d.0.join("result.o");
                for wrong in ["run", "missing"] {
                    let before = d.entries();
                    let out = command(&source, &object, wrong, false);
                    assert_eq!(out.status.code(), Some(2), "{out:?}");
                    assert_eq!(d.entries(), before);
                }
                // Legal neighboring source shapes remain outside the sealed two-root contract.
                for rejected in [
                    text.replace("run(p, c)", "run(1, 2)"),
                    format!("{text}\nfun third() {{}}"),
                ] {
                    fs::write(&source, &rejected).unwrap();
                    let before = d.entries();
                    let out = command(&source, &object, "entry", false);
                    assert_eq!(out.status.code(), Some(2), "{out:?}");
                    assert!(out.stdout.is_empty());
                    assert_eq!(d.entries(), before);
                    assert_eq!(fs::read_to_string(&source).unwrap(), rejected);
                }
                fs::write(&source, text).unwrap();
                let out = command(&source, &object, "entry", false);
                assert!(out.status.success(), "{out:?}");
                assert!(out.stdout.is_empty());
                assert!(out.stderr.is_empty());
                let bytes = fs::read(&object).unwrap();
                assert!(!bytes.is_empty());
                let nm = Command::new("/usr/bin/nm")
                    .args(if cfg!(target_os = "macos") {
                        ["-j", "-U"]
                    } else {
                        ["-j", "--defined-only"]
                    })
                    .arg(&object)
                    .output()
                    .unwrap();
                assert!(nm.status.success());
                assert_eq!(
                    String::from_utf8(nm.stdout)
                        .unwrap()
                        .lines()
                        .filter(|s| *s
                            == if cfg!(target_os = "macos") {
                                "_main"
                            } else {
                                "main"
                            })
                        .count(),
                    1
                );
                let before = d.entries();
                let out = command(&source, &object, "entry", false);
                assert_eq!(out.status.code(), Some(2));
                assert_eq!(fs::read(&object).unwrap(), bytes);
                assert_eq!(d.entries(), before);
            }
        }
    }
    #[test]
    fn real_frontend_failures_and_scope_refusals_publish_nothing() {
        for (source_text, status) in [
            ("fun entry( {", 1),
            ("fun entry() { missing() }", 1),
            ("fun entry() { val a: Int = \"bad\" }", 1),
            (
                "fun entry() { val a = 5; val b = a.fooBar(); println(\"x\") }",
                1,
            ),
            (
                "fun entry() { val a = \"a\"; val b = a; println(a); println(b) }",
                1,
            ),
            ("fun entry() {}", 2),
        ] {
            for json in [false, true] {
                let d = Directory::new();
                let source = d.0.join("source.ko");
                fs::write(&source, source_text).unwrap();
                let object = d.0.join("result.o");
                let before = d.entries();
                let out = command(&source, &object, "entry", json);
                assert_eq!(out.status.code(), Some(status), "{source_text}: {out:?}");
                assert!(out.stdout.is_empty());
                if status == 1 {
                    // Explicit selector errors must not mask actual source admission failures.
                    let missing = command(&source, &object, "absentEntry", json);
                    assert_eq!(missing.status.code(), Some(1));
                    assert!(missing.stdout.is_empty());
                    assert_eq!(missing.stderr, out.stderr);
                }
                assert_eq!(d.entries(), before);
                assert_eq!(fs::read_to_string(&source).unwrap(), source_text);
                if json && status == 1 {
                    let records = String::from_utf8(out.stderr)
                        .unwrap()
                        .lines()
                        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
                        .collect::<Vec<_>>();
                    assert!(!records.is_empty());
                    if source_text.contains("fooBar") {
                        let diagnostic = records.iter().find(|r| r["code"] == "L0080").unwrap();
                        assert_eq!(diagnostic["primary"]["source"], source.to_str().unwrap());
                        let begin = diagnostic["primary"]["byte_start"].as_u64().unwrap() as usize;
                        let end = diagnostic["primary"]["byte_end"].as_u64().unwrap() as usize;
                        assert_eq!(&source_text[begin..end], "fooBar");
                    }
                }
            }
        }
    }
    #[test]
    fn existing_and_aliased_paths_are_never_replaced() {
        let d = Directory::new();
        let source = d.0.join("source.ko");
        fs::write(&source, BEFORE).unwrap();
        let existing = d.0.join("old.o");
        fs::write(&existing, b"existing object bytes").unwrap();
        let hard = d.0.join("hard.o");
        fs::hard_link(&source, &hard).unwrap();
        let sub = d.0.join("sub");
        fs::create_dir(&sub).unwrap();
        for output in [
            &source,
            d.0.join("./source.ko").as_path(),
            sub.join("../source.ko").as_path(),
            &hard,
            &existing,
        ] {
            let before = d.entries();
            let out = command(&source, output, "entry", false);
            assert_eq!(out.status.code(), Some(2));
            assert_eq!(d.entries(), before);
            assert_eq!(fs::read_to_string(&source).unwrap(), BEFORE);
            assert_eq!(fs::read(&existing).unwrap(), b"existing object bytes");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = d.0.join("link.o");
            symlink(&source, &link).unwrap();
            let dangling = d.0.join("dangling.o");
            symlink(d.0.join("absent"), &dangling).unwrap();
            let parent = d.0.join("parent");
            symlink(&d.0, &parent).unwrap();
            for output in [&link, &dangling, &parent.join("source.ko")] {
                let before = d.entries();
                let out = command(&source, output, "entry", false);
                assert_eq!(out.status.code(), Some(2));
                assert_eq!(d.entries(), before);
                assert_eq!(fs::read_to_string(&source).unwrap(), BEFORE);
            }
        }
        let missing = d.0.join("missing/object.o");
        let before = d.entries();
        assert_eq!(
            command(&source, &missing, "entry", false).status.code(),
            Some(2)
        );
        assert_eq!(d.entries(), before);
    }
}
