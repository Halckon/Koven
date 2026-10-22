//! SPEC-0349 `kovenc check` 与 `kovenc --help` 的真实进程验收。

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
            "koven check cli test-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("test directory must be creatable");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn entries(&self) -> Vec<PathBuf> {
        let mut entries = fs::read_dir(&self.0)
            .expect("test directory listing")
            .map(|entry| entry.expect("directory entry").path())
            .collect::<Vec<_>>();
        entries.sort();
        entries
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("owned test directory must be removable");
    }
}

fn kovenc<const N: usize>(arguments: [&OsStr; N]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kovenc"))
        .args(arguments)
        .output()
        .expect("kovenc process must start")
}

const CLEAN: &str = "fun f(): Result<Int, String> = Ok(1)\n\
fun g(flag: Boolean): Result<Int, String> = if (flag) Result.Ok(2) else Err(\"bad\")\n\
fun score(result: Result<Int, String>): Int = when (result) {\n\
    is Ok -> result.success\n\
    is Err -> 0\n\
}\n\
fun main(): Unit {\n\
    val r = g(false)\n\
    if (r is Err) { println(\"err\") }\n\
    if (score(f()) == 1) { println(\"ok\") }\n\
}\n";

#[test]
fn clean_single_file_exits_zero_without_output_or_artifacts() {
    let directory = TestDirectory::create();
    let source = directory.join("clean.ko");
    fs::write(&source, CLEAN).expect("source write");
    let before = directory.entries();
    for format in ["--message-format=human", "--message-format=json"] {
        let output = kovenc([OsStr::new(format), OsStr::new("check"), source.as_os_str()]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
    let rooted = kovenc([
        OsStr::new("check"),
        source.as_os_str(),
        OsStr::new("--source-root"),
        directory.0.as_os_str(),
    ]);
    assert_eq!(rooted.status.code(), Some(0), "{rooted:?}");
    // check 不选择入口：没有 main 的文件同样通过。
    let library = directory.join("library.ko");
    fs::write(&library, "fun helper(): Result<Int, Int> = Err(2)\n").expect("source write");
    let library_output = kovenc([OsStr::new("check"), library.as_os_str()]);
    assert_eq!(library_output.status.code(), Some(0), "{library_output:?}");
    let mut expected = before;
    expected.push(library);
    expected.sort();
    assert_eq!(
        directory.entries(),
        expected,
        "check must not write artifacts"
    );
}

#[test]
fn diagnostics_exit_one_and_render_like_build() {
    let directory = TestDirectory::create();
    let source = directory.join("broken.ko");
    fs::write(&source, "fun main(): Unit { missing }\n").expect("source write");
    let human = kovenc([OsStr::new("check"), source.as_os_str()]);
    assert_eq!(human.status.code(), Some(1), "{human:?}");
    assert!(human.stdout.is_empty());
    let human_text = String::from_utf8(human.stderr.clone()).expect("UTF-8 diagnostics");
    assert!(human_text.starts_with("error[L0080] "), "{human_text}");

    let built = kovenc([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        directory.join("program").as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(1), "{built:?}");
    assert_eq!(
        built.stderr, human.stderr,
        "check renders exactly like build"
    );

    let json = kovenc([
        OsStr::new("--message-format=json"),
        OsStr::new("check"),
        source.as_os_str(),
    ]);
    assert_eq!(json.status.code(), Some(1), "{json:?}");
    let records = String::from_utf8(json.stderr)
        .expect("JSON Lines")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON record"))
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0]["code"], "L0080");

    let conflict = directory.join("conflict.ko");
    fs::write(&conflict, "fun Ok(): Unit {}\n").expect("source write");
    let conflicted = kovenc([OsStr::new("check"), conflict.as_os_str()]);
    assert_eq!(conflicted.status.code(), Some(1), "{conflicted:?}");
    assert!(
        String::from_utf8_lossy(&conflicted.stderr).starts_with("error[L0150] "),
        "{conflicted:?}"
    );
    assert!(!directory.join("program").exists());
}

#[test]
fn usage_and_operational_errors_exit_two() {
    let directory = TestDirectory::create();
    for arguments in [
        vec![OsStr::new("check")],
        vec![OsStr::new("check"), OsStr::new("--unknown")],
        vec![OsStr::new("check"), OsStr::new("--project")],
        vec![
            OsStr::new("check"),
            OsStr::new("a.ko"),
            OsStr::new("--entry"),
            OsStr::new("main"),
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_kovenc"))
            .args(&arguments)
            .output()
            .expect("kovenc process must start");
        assert_eq!(output.status.code(), Some(2), "{arguments:?}: {output:?}");
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("usage: kovenc check"),
            "{output:?}"
        );
    }
    let missing = directory.join("missing.ko");
    let unreadable = kovenc([OsStr::new("check"), missing.as_os_str()]);
    assert_eq!(unreadable.status.code(), Some(2), "{unreadable:?}");
    assert!(
        String::from_utf8_lossy(&unreadable.stderr).starts_with("error: check failed: cannot read"),
        "{unreadable:?}"
    );
    let manifest = directory.join("missing.toml");
    let project = kovenc([
        OsStr::new("check"),
        OsStr::new("--project"),
        manifest.as_os_str(),
    ]);
    assert_eq!(project.status.code(), Some(2), "{project:?}");
    assert!(
        String::from_utf8_lossy(&project.stderr)
            .starts_with("error: check failed: project load failed"),
        "{project:?}"
    );
}

#[test]
fn project_mode_checks_the_whole_unit() {
    let directory = TestDirectory::create();
    fs::create_dir_all(directory.join("src/app")).expect("project source root");
    fs::write(
        directory.join("src/app/Main.ko"),
        format!("package app\n{CLEAN}"),
    )
    .expect("project source write");
    let manifest = directory.join("project.toml");
    fs::write(
        &manifest,
        "schema = \"koven.project\"\nversion = 1\n\n[project]\nname = \"check-project\"\nsource-roots = [\"src\"]\n",
    )
    .expect("project manifest write");
    let clean = kovenc([
        OsStr::new("check"),
        OsStr::new("--project"),
        manifest.as_os_str(),
    ]);
    assert_eq!(clean.status.code(), Some(0), "{clean:?}");
    assert!(
        clean.stdout.is_empty() && clean.stderr.is_empty(),
        "{clean:?}"
    );

    fs::write(
        directory.join("src/app/Other.ko"),
        "package app\nenum class Result { Only }\n",
    )
    .expect("conflicting source write");
    let conflicted = kovenc([
        OsStr::new("--message-format=json"),
        OsStr::new("check"),
        OsStr::new("--project"),
        manifest.as_os_str(),
    ]);
    assert_eq!(conflicted.status.code(), Some(1), "{conflicted:?}");
    let records = String::from_utf8(conflicted.stderr)
        .expect("JSON Lines")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON record"))
        .collect::<Vec<_>>();
    assert!(
        records.iter().any(|record| record["code"] == "L0150"),
        "{records:?}"
    );
}

#[test]
fn help_prints_full_usage_to_stdout() {
    for spelling in ["--help", "-h", "help"] {
        let output = kovenc([OsStr::new(spelling)]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let text = String::from_utf8(output.stdout).expect("UTF-8 usage");
        assert!(text.starts_with("usage: kovenc"), "{text}");
        for form in [
            "kovenc check <source.ko> [--source-root <dir>]",
            "kovenc check --project <project.toml>",
            "kovenc build --project <project.toml>",
            "kovenc run <source.ko>",
            "kovenc format [--check] <path>",
        ] {
            assert!(text.contains(form), "{form}: {text}");
        }
    }
    let unknown = kovenc([OsStr::new("unknown")]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&unknown.stderr)
            .contains("expected `format`, `check`, `build`, `run` or `help`")
    );
}
