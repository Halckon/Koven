//! Explicit two-source experimental command. Native execution is a separate exact test.
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
            "koven-d10-split-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn entries(&self) -> Vec<PathBuf> {
        let mut entries = fs::read_dir(&self.0)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        entries.sort();
        entries
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("D10 split failure retained at {}", self.0.display());
        } else {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn command(first: &Path, second: &Path, output: &Path, entry: &str, json: bool) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_kovenc"));
    if json {
        c.arg("--message-format=json");
    }
    c.arg("experimental-d10-split-object")
        .arg(first)
        .arg(second)
        .args(["--entry", entry, "-o"])
        .arg(output)
        .output()
        .unwrap()
}
#[cfg(not(feature = "d10-catalog-experiment"))]
#[test]
fn default_cli_never_selects_split_experiment() {
    let d = Directory::new();
    let a = d.0.join("a.ko");
    let b = d.0.join("b.ko");
    fs::write(&a, "fun entry() {}").unwrap();
    fs::write(&b, "fun run() {}").unwrap();
    let before = d.entries();
    let output = command(&a, &b, &d.0.join("object.o"), "entry", false);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(d.entries(), before);
    let help = Command::new(env!("CARGO_BIN_EXE_kovenc"))
        .arg("help")
        .output()
        .unwrap();
    assert!(
        !String::from_utf8(help.stdout)
            .unwrap()
            .contains("experimental-d10-split-object")
    );
}
#[cfg(feature = "d10-catalog-experiment")]
mod enabled {
    use super::*;
    const BODIES: [&str; 2] = [
        "val parentOld = replace(&a, \"parent\"); var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; outer -= 1",
        "var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; val parentOld = replace(&a, \"parent\"); outer -= 1",
    ];
    fn source_pair(
        d: &Directory,
        order: usize,
        callee_first: bool,
        parent: u32,
        child: u32,
    ) -> (PathBuf, PathBuf) {
        let (callee, entry) = if callee_first {
            ("a.ko", "z.ko")
        } else {
            ("z.ko", "a.ko")
        };
        let callee = d.0.join(callee);
        let entry = d.0.join(entry);
        fs::write(&callee, format!("fun run(count: Int, childCount: Int) {{ var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) {{ {} }}; println(a); println(b) }}", BODIES[order])).unwrap();
        fs::write(
            &entry,
            format!("fun entry() {{ val p = {parent}; val c = {child}; run(p, c) }}"),
        )
        .unwrap();
        (callee, entry)
    }
    #[test]
    fn split_cli_objects_preserve_sources_selectors_and_existing_targets() {
        for order in 0..2 {
            for callee_first in [false, true] {
                for reversed in [false, true] {
                    let d = Directory::new();
                    let (callee, entry) = source_pair(&d, order, callee_first, 1, 2);
                    let (first, second) = if reversed {
                        (&entry, &callee)
                    } else {
                        (&callee, &entry)
                    };
                    let object = d.0.join("entry.o");
                    let callee_bytes = fs::read(&callee).unwrap();
                    let entry_bytes = fs::read(&entry).unwrap();
                    for wrong in ["run", "missing"] {
                        let before = d.entries();
                        let output = command(first, second, &object, wrong, false);
                        assert_eq!(output.status.code(), Some(2), "{output:?}");
                        assert_eq!(d.entries(), before);
                    }
                    let output = command(first, second, &object, "entry", false);
                    assert!(output.status.success(), "{output:?}");
                    assert!(output.stdout.is_empty());
                    assert!(output.stderr.is_empty());
                    let bytes = fs::read(&object).unwrap();
                    assert!(!bytes.is_empty());
                    let before = d.entries();
                    let output = command(first, second, &object, "entry", false);
                    assert_eq!(output.status.code(), Some(2));
                    assert_eq!(fs::read(&object).unwrap(), bytes);
                    assert_eq!(d.entries(), before);
                    for source in [&callee, &entry] {
                        assert_eq!(
                            command(first, second, source, "entry", false).status.code(),
                            Some(2)
                        );
                    }
                    assert_eq!(fs::read(&callee).unwrap(), callee_bytes);
                    assert_eq!(fs::read(&entry).unwrap(), entry_bytes);
                    assert_eq!(
                        command(&callee, &callee, &d.0.join("duplicate.o"), "entry", false)
                            .status
                            .code(),
                        Some(2)
                    );
                    #[cfg(unix)]
                    {
                        let alias = d.0.join("alias.ko");
                        std::os::unix::fs::symlink(&callee, &alias).unwrap();
                        let before = d.entries();
                        assert_eq!(
                            command(&callee, &alias, &d.0.join("alias.o"), "entry", false)
                                .status
                                .code(),
                            Some(2)
                        );
                        assert_eq!(d.entries(), before);
                    }
                }
            }
        }
    }
    #[test]
    fn split_cli_language_errors_precede_selector_and_publish_nothing() {
        for (text, status) in [
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
            ("fun entry() { run(1, 2) }", 2),
            (
                "fun entry() { val p = 1; val c = 2; run(p, c) }; fun extra() {}",
                2,
            ),
        ] {
            for json in [false, true] {
                let d = Directory::new();
                let (callee, entry) = source_pair(&d, 0, true, 1, 2);
                fs::write(&entry, text).unwrap();
                let before = d.entries();
                let object = d.0.join("entry.o");
                let output = command(&callee, &entry, &object, "entry", json);
                assert_eq!(output.status.code(), Some(status), "{text}: {output:?}");
                assert!(output.stdout.is_empty());
                assert_eq!(d.entries(), before);
                if status == 1 {
                    let missing = command(&callee, &entry, &object, "missingEntry", json);
                    assert_eq!(missing.status.code(), Some(1));
                    assert_eq!(missing.stderr, output.stderr);
                }
                if json && status == 1 {
                    let records = std::str::from_utf8(&output.stderr)
                        .unwrap()
                        .lines()
                        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
                        .collect::<Vec<_>>();
                    assert!(!records.is_empty());
                    if text.contains("fooBar") {
                        let diagnostic = records
                            .iter()
                            .find(|record| record["code"] == "L0080")
                            .unwrap();
                        assert_eq!(diagnostic["primary"]["source"], entry.to_str().unwrap());
                        let begin = diagnostic["primary"]["byte_start"].as_u64().unwrap() as usize;
                        let end = diagnostic["primary"]["byte_end"].as_u64().unwrap() as usize;
                        assert_eq!(&text[begin..end], "fooBar");
                    }
                }
                assert_eq!(fs::read_to_string(&entry).unwrap(), text);
            }
        }
    }
    #[test]
    fn split_cli_native_zero_and_multiple_rounds() {
        use std::{
            process::Stdio,
            time::{Duration, Instant},
        };
        for order in 0..2 {
            for callee_first in [false, true] {
                for reversed in [false, true] {
                    for (parent, child) in [(0, 0), (0, 2), (2, 0), (2, 3)] {
                        let d = Directory::new();
                        let (callee, entry) = source_pair(&d, order, callee_first, parent, child);
                        let (first, second) = if reversed {
                            (&entry, &callee)
                        } else {
                            (&callee, &entry)
                        };
                        let object = d.0.join("entry.o");
                        let executable = d.0.join("program");
                        let output = command(first, second, &object, "entry", false);
                        assert!(output.status.success(), "{output:?}");
                        assert!(output.stderr.is_empty());
                        let linked = Command::new("cc")
                            .arg(&object)
                            .arg("-o")
                            .arg(&executable)
                            .output()
                            .unwrap();
                        assert!(linked.status.success(), "{linked:?}");
                        let expected = if parent == 0 {
                            "first\nkept\n"
                        } else if order == 0 && child > 0 {
                            "next\nkept\n"
                        } else {
                            "parent\nkept\n"
                        };
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
                                let _ = process.kill();
                                let output = process.wait_with_output().unwrap();
                                panic!(
                                    "D10 split native deadline order={order} callee_first={callee_first} reversed={reversed} parent={parent} child={child}: {output:?}"
                                );
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        let output = process.wait_with_output().unwrap();
                        assert!(output.status.success(), "{output:?}");
                        assert_eq!(output.stdout, expected.as_bytes());
                        assert!(output.stderr.is_empty(), "{output:?}");
                    }
                }
            }
        }
    }
}
