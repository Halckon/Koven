//! SPEC-0057/0060 真实 `kovenc format` 进程与诊断展示边界。

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
            "koven formatter test-{}-{}",
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
fn format_writes_stdout_and_check_uses_zero_or_one_without_mutating_input() {
    let directory = TestDirectory::create();
    let source_path = directory.join("source.ko");
    let original = b"fun  entry():Unit {}";
    fs::write(&source_path, original).expect("source write");

    let formatted = run([OsStr::new("format"), source_path.as_os_str()]);
    assert_eq!(formatted.status.code(), Some(0));
    assert_eq!(formatted.stdout, b"fun entry(): Unit {}");
    assert!(formatted.stderr.is_empty());
    assert_eq!(fs::read(&source_path).expect("source read"), original);

    let differs = run([
        OsStr::new("format"),
        OsStr::new("--check"),
        source_path.as_os_str(),
    ]);
    assert_eq!(differs.status.code(), Some(1));
    assert!(differs.stdout.is_empty());
    assert!(differs.stderr.is_empty());
    assert_eq!(fs::read(&source_path).expect("source read"), original);

    fs::write(&source_path, &formatted.stdout).expect("canonical source write");
    let canonical = run([
        OsStr::new("format"),
        OsStr::new("--check"),
        source_path.as_os_str(),
    ]);
    assert_eq!(canonical.status.code(), Some(0));
    assert!(canonical.stdout.is_empty());
    assert!(canonical.stderr.is_empty());
}

#[test]
fn arguments_io_utf8_and_frontend_diagnostics_exit_two() {
    let directory = TestDirectory::create();
    let invalid = directory.join("invalid.ko");
    fs::write(&invalid, b"val item = $").expect("invalid source write");
    let diagnostic = run([OsStr::new("format"), invalid.as_os_str()]);
    assert_eq!(diagnostic.status.code(), Some(2));
    assert!(diagnostic.stdout.is_empty());
    assert!(
        String::from_utf8(diagnostic.stderr)
            .expect("UTF-8 diagnostic")
            .contains("error[L0001]")
    );

    let missing = run([OsStr::new("format")]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(
        String::from_utf8(missing.stderr)
            .expect("UTF-8 usage")
            .contains("expected exactly one source path")
    );

    let unknown = run([
        OsStr::new("format"),
        OsStr::new("--write"),
        invalid.as_os_str(),
    ]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(
        String::from_utf8(unknown.stderr)
            .expect("UTF-8 usage")
            .contains("unknown format option --write")
    );

    let absent = directory.join("absent.ko");
    let unreadable = run([OsStr::new("format"), absent.as_os_str()]);
    assert_eq!(unreadable.status.code(), Some(2));
    assert!(
        String::from_utf8(unreadable.stderr)
            .expect("UTF-8 IO error")
            .contains("cannot read")
    );

    let non_utf8 = directory.join("non-utf8.ko");
    fs::write(&non_utf8, [0xff, 0xfe]).expect("non-UTF-8 source write");
    let rejected = run([OsStr::new("format"), non_utf8.as_os_str()]);
    assert_eq!(rejected.status.code(), Some(2));
    assert!(
        String::from_utf8(rejected.stderr)
            .expect("UTF-8 error")
            .contains("is not valid UTF-8")
    );
}

#[test]
fn machine_diagnostics_are_json_lines_without_mixing_formatter_stdout() {
    let directory = TestDirectory::create();
    let invalid = directory.join("invalid machine.ko");
    fs::write(&invalid, b"val item = $").expect("invalid source write");

    let machine = run([
        OsStr::new("--message-format=json"),
        OsStr::new("format"),
        invalid.as_os_str(),
    ]);
    assert_eq!(machine.status.code(), Some(2));
    assert!(machine.stdout.is_empty());
    let stderr = String::from_utf8(machine.stderr).expect("UTF-8 JSON Lines");
    let records = stderr
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("one JSON object"))
        .collect::<Vec<_>>();
    assert!(!records.is_empty());
    assert!(records.iter().all(|record| {
        record["schema"] == "koven.diagnostic"
            && record["version"] == 1
            && record["primary"]["source"] == invalid.to_string_lossy().as_ref()
    }));
    assert!(records.iter().any(|record| record["code"] == "L0001"));

    let human = run([
        OsStr::new("--message-format=human"),
        OsStr::new("format"),
        invalid.as_os_str(),
    ]);
    assert_eq!(human.status.code(), Some(2));
    assert!(human.stdout.is_empty());
    assert!(
        String::from_utf8(human.stderr)
            .expect("UTF-8 human diagnostic")
            .contains("error[L0001]")
    );

    let valid = directory.join("valid.ko");
    fs::write(&valid, b"fun  entry():Unit {}").expect("valid source write");
    let formatted = run([
        OsStr::new("--message-format=json"),
        OsStr::new("format"),
        valid.as_os_str(),
    ]);
    assert_eq!(formatted.status.code(), Some(0));
    assert_eq!(formatted.stdout, b"fun entry(): Unit {}");
    assert!(formatted.stderr.is_empty());

    let misplaced = run([
        OsStr::new("format"),
        OsStr::new("--message-format=json"),
        valid.as_os_str(),
    ]);
    assert_eq!(misplaced.status.code(), Some(2));
    assert!(misplaced.stdout.is_empty());
    assert!(
        String::from_utf8(misplaced.stderr)
            .expect("UTF-8 usage error")
            .contains("unknown format option --message-format=json")
    );
}

fn run<const N: usize>(arguments: [&OsStr; N]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kovenc"))
        .args(arguments)
        .output()
        .expect("kovenc process must start")
}
