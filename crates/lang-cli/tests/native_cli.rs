//! SPEC-0190 真实 `kovenc build/run` 单文件进程验收。

use std::{
    ffi::OsStr,
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven native cli test-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("test directory must be creatable");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("owned test directory must be removable");
    }
}

#[test]
fn public_build_and_run_execute_an_external_hello_world() {
    let directory = TestDirectory::create();
    let source = directory.join("hello.ko");
    let executable = directory.join("hello");
    fs::write(
        &source,
        "fun hello(): Unit { println(\"Hello, World!\") }\n",
    )
    .expect("source write");

    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("hello"),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    assert!(built.stdout.is_empty());
    assert!(built.stderr.is_empty());
    assert!(executable.is_file());
    assert_eq!(
        fs::read_dir(&directory.0)
            .expect("directory read")
            .filter_map(Result::ok)
            .filter(|entry| entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "o"))
            .count(),
        0,
        "public build must clean its temporary object"
    );
    let launched = Command::new(&executable)
        .output()
        .expect("executable launch");
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(launched.stdout, b"Hello, World!\n");
    assert!(launched.stderr.is_empty());

    let executed = run([
        OsStr::new("run"),
        source.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("hello"),
    ]);
    assert_eq!(executed.status.code(), Some(0), "{executed:?}");
    assert_eq!(executed.stdout, b"Hello, World!\n");
    assert!(executed.stderr.is_empty());
}

#[test]
fn conventional_main_builds_and_runs_without_an_entry_option() {
    let directory = TestDirectory::create();
    let source = directory.join("main.ko");
    let executable = directory.join("main");
    fs::write(&source, "fun main(): Unit { println(\"Hello, World!\") }\n").expect("source write");

    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    assert!(built.stdout.is_empty());
    assert!(built.stderr.is_empty());
    let launched = Command::new(&executable)
        .output()
        .expect("executable launch");
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(launched.stdout, b"Hello, World!\n");
    assert!(launched.stderr.is_empty());

    let executed = run([OsStr::new("run"), source.as_os_str()]);
    assert_eq!(executed.status.code(), Some(0), "{executed:?}");
    assert_eq!(executed.stdout, b"Hello, World!\n");
    assert!(executed.stderr.is_empty());
}

#[test]
fn conventional_main_builds_and_runs_intrinsic_rc_owners() {
    let directory = TestDirectory::create();
    let source = directory.join("shared.ko");
    let executable = directory.join("shared");
    fs::write(
        &source,
        "value class Point(val x: Int)\n\
         fun main(): Unit {\n\
             val first = Rc(Point(41))\n\
             val second = first.share()\n\
             val third = second.share()\n\
             val number = Rc(41)\n\
             val copied = number.value\n\
             if (copied != 41) { error(\"bad shared payload\") }\n\
             println(\"shared\")\n\
         }\n",
    )
    .expect("source write");

    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    assert!(built.stdout.is_empty());
    assert!(built.stderr.is_empty());
    let launched = Command::new(&executable)
        .output()
        .expect("executable launch");
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(launched.stdout, b"shared\n");
    assert!(launched.stderr.is_empty());

    let executed = run([OsStr::new("run"), source.as_os_str()]);
    assert_eq!(executed.status.code(), Some(0), "{executed:?}");
    assert_eq!(executed.stdout, b"shared\n");
    assert!(executed.stderr.is_empty());
}

#[test]
fn conventional_main_reports_selection_failures_and_explicit_entry_still_wins() {
    let directory = TestDirectory::create();

    let missing_source = directory.join("missing.ko");
    fs::write(&missing_source, "fun helper(): Unit {}\n").expect("missing source write");
    let missing = run([OsStr::new("run"), missing_source.as_os_str()]);
    assert_eq!(missing.status.code(), Some(2), "{missing:?}");
    assert!(String::from_utf8_lossy(&missing.stderr).contains("entry `main` was not found"));

    let invalid_source = directory.join("invalid-shape.ko");
    fs::write(
        &invalid_source,
        "fun main(args: Array<String>): Unit {}\nfun selected(): Unit {}\n",
    )
    .expect("invalid-shape source write");
    let invalid = run([OsStr::new("run"), invalid_source.as_os_str()]);
    assert_eq!(invalid.status.code(), Some(2), "{invalid:?}");
    assert!(
        String::from_utf8_lossy(&invalid.stderr)
            .contains("parameterized entry `main` requires argv support")
    );
    fs::write(
        &invalid_source,
        "fun main(): Int { return 1 }\nfun selected(): Unit {}\n",
    )
    .expect("explicit override source write");
    let invalid = run([OsStr::new("run"), invalid_source.as_os_str()]);
    assert_eq!(invalid.status.code(), Some(2), "{invalid:?}");
    assert!(
        String::from_utf8_lossy(&invalid.stderr)
            .contains("entry `main` has no supported conventional shape")
    );
    let explicit = run([
        OsStr::new("run"),
        invalid_source.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("selected"),
    ]);
    assert_eq!(explicit.status.code(), Some(0), "{explicit:?}");
    assert!(explicit.stdout.is_empty());
    assert!(explicit.stderr.is_empty());

    let ambiguous_source = directory.join("ambiguous.ko");
    fs::write(
        &ambiguous_source,
        "fun main(): Unit {}\nfun main(args: Array<String>): Unit {}\n",
    )
    .expect("ambiguous source write");
    let ambiguous = run([OsStr::new("run"), ambiguous_source.as_os_str()]);
    assert_eq!(ambiguous.status.code(), Some(2), "{ambiguous:?}");
    assert!(
        String::from_utf8_lossy(&ambiguous.stderr)
            .contains("entry `main` is ambiguous (2 candidates)"),
        "{ambiguous:?}"
    );
}

#[test]
fn native_commands_reject_usage_outputs_entries_and_frontend_errors() {
    let directory = TestDirectory::create();
    let source = directory.join("source.ko");
    let executable = directory.join("program");
    fs::write(&source, "fun present(): Unit {}\n").expect("source write");

    let usage = run([OsStr::new("build"), source.as_os_str()]);
    assert_eq!(usage.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&usage.stderr).contains("usage: kovenc build"));

    fs::write(&executable, b"caller owned").expect("existing output write");
    let exists = build(source.as_os_str(), "present", executable.as_os_str());
    assert_eq!(exists.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&exists.stderr).contains("output already exists"));
    assert_eq!(fs::read(&executable).expect("output read"), b"caller owned");

    let missing_output = directory.join("missing-entry");
    let missing = build(source.as_os_str(), "absent", missing_output.as_os_str());
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("entry `absent` was not found"));
    assert!(!missing_output.exists());

    let invalid = directory.join("invalid.ko");
    fs::write(&invalid, "fun broken(): Unit { missing }\n").expect("invalid source write");
    let human_output = directory.join("human");
    let human = build(invalid.as_os_str(), "broken", human_output.as_os_str());
    assert_eq!(human.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&human.stderr).contains("error[L0080]"));
    assert!(!human_output.exists());

    let json_output = directory.join("json");
    let json = run([
        OsStr::new("--message-format=json"),
        OsStr::new("build"),
        invalid.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("broken"),
        OsStr::new("-o"),
        json_output.as_os_str(),
    ]);
    assert_eq!(json.status.code(), Some(2));
    assert!(json.stdout.is_empty());
    let records = String::from_utf8(json.stderr)
        .expect("JSON Lines")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON record"))
        .collect::<Vec<_>>();
    assert!(records.iter().any(|record| record["code"] == "L0080"));
    assert!(!json_output.exists());
}

fn build(source: &OsStr, entry: &str, executable: &OsStr) -> Output {
    run([
        OsStr::new("build"),
        source,
        OsStr::new("--entry"),
        OsStr::new(entry),
        OsStr::new("-o"),
        executable,
    ])
}

fn run<const N: usize>(arguments: [&OsStr; N]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kovenc"))
        .args(arguments)
        .output()
        .expect("kovenc process must start")
}
