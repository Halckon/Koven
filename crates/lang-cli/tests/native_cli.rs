//! SPEC-0190 真实 `kovenc build/run` 单文件进程验收。

#[path = "native_cli/runtime_constructor.rs"]
mod runtime_constructor;

use std::{
    ffi::OsStr,
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(unix)]
use std::{ffi::OsString, os::unix::ffi::OsStringExt};

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

    let ignored = run([
        OsStr::new("run"),
        source.as_os_str(),
        OsStr::new("--"),
        OsStr::new("ignored"),
        OsStr::new(""),
        OsStr::new("忽略"),
    ]);
    assert_eq!(ignored.status.code(), Some(0), "{ignored:?}");
    assert_eq!(ignored.stdout, b"Hello, World!\n");
    assert!(ignored.stderr.is_empty(), "{ignored:?}");
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

    let parameterized_source = directory.join("explicit-parameterized.ko");
    fs::write(
        &parameterized_source,
        "fun main(args: Array<String>): Unit {}\n",
    )
    .expect("explicit parameterized source write");
    let explicit_parameterized = run([
        OsStr::new("run"),
        parameterized_source.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("main"),
        OsStr::new("--"),
        OsStr::new("ignored"),
    ]);
    assert_eq!(
        explicit_parameterized.status.code(),
        Some(2),
        "{explicit_parameterized:?}"
    );
    assert!(
        String::from_utf8_lossy(&explicit_parameterized.stderr).contains("InvalidEntry"),
        "{explicit_parameterized:?}"
    );

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
fn parameterized_main_preserves_argument_bytes_and_order_for_build_and_run() {
    let directory = TestDirectory::create();
    let source = directory.join("arguments.ko");
    let executable = directory.join("arguments");
    fs::write(
        &source,
        "fun main(args: Array<String>): Unit {\n\
             val first = println(args[0])\n\
             val empty = println(args[1])\n\
             val unicode = println(args[2])\n\
             val last = println(args[3])\n\
         }\n\
         fun selected(): Unit { println(\"explicit\") }\n",
    )
    .expect("parameterized source write");

    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    assert!(built.stdout.is_empty(), "{built:?}");
    assert!(built.stderr.is_empty(), "{built:?}");
    let launched = Command::new(&executable)
        .args(["first", "", "你好", "last"])
        .output()
        .expect("parameterized executable launch");
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(launched.stdout, "first\n\n你好\nlast\n".as_bytes());
    assert!(launched.stderr.is_empty(), "{launched:?}");

    let ordered = run([
        OsStr::new("run"),
        source.as_os_str(),
        OsStr::new("--"),
        OsStr::new("first"),
        OsStr::new(""),
        OsStr::new("你好"),
        OsStr::new("last"),
    ]);
    assert_eq!(ordered.status.code(), Some(0), "{ordered:?}");
    assert_eq!(ordered.stdout, "first\n\n你好\nlast\n".as_bytes());
    assert!(ordered.stderr.is_empty(), "{ordered:?}");

    let explicit = run([
        OsStr::new("run"),
        source.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("selected"),
        OsStr::new("--"),
        OsStr::new("ignored-first"),
        OsStr::new(""),
        OsStr::new("忽略"),
    ]);
    assert_eq!(explicit.status.code(), Some(0), "{explicit:?}");
    assert_eq!(explicit.stdout, b"explicit\n");
    assert!(explicit.stderr.is_empty(), "{explicit:?}");
}

#[test]
fn parameterized_main_accepts_an_empty_argument_array() {
    let directory = TestDirectory::create();
    let source = directory.join("empty-arguments.ko");
    fs::write(
        &source,
        "fun main(args: Array<String>): Unit { println(\"empty\") }\n",
    )
    .expect("empty parameterized source write");

    let empty = run([OsStr::new("run"), source.as_os_str(), OsStr::new("--")]);
    assert_eq!(empty.status.code(), Some(0), "{empty:?}");
    assert_eq!(empty.stdout, b"empty\n");
    assert!(empty.stderr.is_empty(), "{empty:?}");
}

#[cfg(unix)]
#[test]
fn parameterized_main_rejects_invalid_utf8_before_entering_koven() {
    let directory = TestDirectory::create();
    let source = directory.join("invalid-utf8.ko");
    fs::write(
        &source,
        "fun main(args: Array<String>): Unit { println(\"entered\") }\n",
    )
    .expect("parameterized source write");
    let invalid = OsString::from_vec(vec![b'v', 0xff, b'x']);

    let rejected = run([
        OsStr::new("run"),
        source.as_os_str(),
        OsStr::new("--"),
        invalid.as_os_str(),
    ]);
    assert_eq!(rejected.status.code(), Some(1), "{rejected:?}");
    assert!(rejected.stdout.is_empty(), "{rejected:?}");
    assert!(rejected.stderr.is_empty(), "{rejected:?}");
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

    let run_usage = run([
        OsStr::new("run"),
        source.as_os_str(),
        OsStr::new("program-argument-without-separator"),
    ]);
    assert_eq!(run_usage.status.code(), Some(2), "{run_usage:?}");
    assert!(
        String::from_utf8_lossy(&run_usage.stderr).contains("usage: kovenc run"),
        "{run_usage:?}"
    );

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

#[test]
fn string_clone_builds_and_runs_through_public_cli() {
    let directory = TestDirectory::create();
    let source = directory.join("clone.ko");
    let executable = directory.join("clone");
    fs::write(
        &source,
        r#"fun duplicate(text: String): String = text.clone()
fun main(): Unit {
    val source = "界\0" + "é"
    val copy = duplicate(source)
    println(source)
    println(copy)
    println("".clone())
}"#,
    )
    .unwrap();
    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert!(built.status.success(), "{built:?}");
    assert!(built.stdout.is_empty() && built.stderr.is_empty());
    let launched = Command::new(&executable).output().unwrap();
    assert!(launched.status.success(), "{launched:?}");
    assert_eq!(launched.stdout, "界\0é\n界\0é\n\n".as_bytes());
    let executed = run([OsStr::new("run"), source.as_os_str()]);
    assert!(executed.status.success(), "{executed:?}");
    assert_eq!(executed.stdout, launched.stdout);
}
