//! SPEC-0252：旧 CLI project typed/basic/const 分流的完整进程输出 oracle。
//!
//! Fixtures 与 golden bytes 在生产迁移前由 main 15dfb13 的 kovenc 实际捕获。
//! 原 project_cli/native_cli oracle 保持独立；本域同时锁 stdout/stderr/exit、
//! 全部诊断字段/顺序、失败不发布产物，以及 basic/const 的 entry gate 次序。

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

type Source = (&'static str, &'static str);

const ENTRY: Source = (
    "aroot/app/Entry.ko",
    include_str!("fixtures/project_basic_ownership/Entry.ko"),
);
const TYPED: [Source; 2] = [
    (
        "zroot/z/Stage.ko",
        include_str!("fixtures/project_basic_ownership/TypedZ.ko"),
    ),
    (
        "aroot/a/Stage.ko",
        include_str!("fixtures/project_basic_ownership/TypedA.ko"),
    ),
];
const MOVED: [Source; 2] = [
    (
        "zroot/z/Stage.ko",
        include_str!("fixtures/project_basic_ownership/MovedZ.ko"),
    ),
    (
        "aroot/a/Stage.ko",
        include_str!("fixtures/project_basic_ownership/MovedA.ko"),
    ),
];
const DEFERRED: Source = (
    "aroot/app/Deferred.ko",
    include_str!("fixtures/project_basic_ownership/Deferred.ko"),
);
const UNUSED_CONSTANT: Source = (
    "aroot/constants/Values.ko",
    include_str!("fixtures/project_basic_ownership/UnusedConstant.ko"),
);
const INVALID_CONSTANT: Source = (
    "aroot/constants/Values.ko",
    include_str!("fixtures/project_basic_ownership/InvalidConstant.ko"),
);
const USED_CONSTANTS: Source = (
    "aroot/constants/Values.ko",
    include_str!("fixtures/project_basic_ownership/UsedConstants.ko"),
);
const CONSTANT_ENTRY: Source = (
    "aroot/app/Entry.ko",
    include_str!("fixtures/project_basic_ownership/ConstantEntry.ko"),
);

const TYPED_HUMAN: &str = include_str!("fixtures/project_basic_ownership/typed.human.stderr");
const TYPED_JSON: &str = include_str!("fixtures/project_basic_ownership/typed.json.stderr");
const MOVED_HUMAN: &str = include_str!("fixtures/project_basic_ownership/moved.human.stderr");
const MOVED_JSON: &str = include_str!("fixtures/project_basic_ownership/moved.json.stderr");
const CONST_TYPED_HUMAN: &str =
    include_str!("fixtures/project_basic_ownership/const_typed.human.stderr");
const CONST_TYPED_JSON: &str =
    include_str!("fixtures/project_basic_ownership/const_typed.json.stderr");
const INCOMPLETE_OWNERSHIP: &str =
    "error: project build failed: ownership analysis contains deferred codegen facts\n";
const MISSING_ENTRY: &str =
    "error: project build failed: project entry `app.absent` was not found\n";
const INVALID_ENTRY: &str = "error: project build failed: project entry `app.invalid` has no supported process shape; expected `fun invalid(): Unit` or `fun invalid(args: Array<String>): Unit`\n";

#[test]
fn project_typed_diagnostics_precede_basic_ownership_and_entry_with_complete_output() {
    let project = Project::create("typed", &TYPED);
    assert_gate(&project, TYPED_HUMAN, TYPED_JSON);
}

#[test]
fn project_basic_ownership_diagnostics_precede_entry_with_complete_output() {
    let project = Project::create("moved", &MOVED);
    assert_gate(&project, MOVED_HUMAN, MOVED_JSON);
}

#[test]
fn project_basic_deferred_ownership_precedes_valid_invalid_and_missing_entry() {
    let project = Project::create("deferred", &[DEFERRED]);
    // 此 fixture 没有语言诊断；raw ownership 的 deferred 不能越过 validate。
    assert_gate(&project, INCOMPLETE_OWNERSHIP, INCOMPLETE_OWNERSHIP);
}

#[test]
fn project_constant_type_diagnostics_precede_independent_ownership_errors() {
    let project = Project::create("const-typed", &[MOVED[0], MOVED[1], INVALID_CONSTANT]);
    assert_gate(&project, CONST_TYPED_HUMAN, CONST_TYPED_JSON);
}

#[test]
fn project_unused_constant_still_checks_independent_use_after_move() {
    let project = Project::create("const-unused-moved", &[MOVED[0], MOVED[1], UNUSED_CONSTANT]);
    // 即使从未读取 const，basic typed capability 仍拒绝整个 unit；CLI 的
    // 专用 const ownership 必须继续发布独立函数中的 L0131，不能套用 LSP 策略。
    assert_gate(&project, MOVED_HUMAN, MOVED_JSON);
}

#[test]
fn project_constant_deferred_ownership_precedes_valid_invalid_and_missing_entry() {
    let project = Project::create("const-deferred", &[DEFERRED, UNUSED_CONSTANT]);
    assert_gate(&project, INCOMPLETE_OWNERSHIP, INCOMPLETE_OWNERSHIP);
}

#[test]
fn project_basic_success_and_entry_failures_preserve_complete_process_output() {
    let project = Project::create("basic-success", &[]);
    assert_success(&project, "app.start", &[], "基础完成\n");
    assert_entry_failures(&project);
}

#[test]
fn project_unused_constant_success_and_entry_failures_preserve_complete_process_output() {
    let project = Project::create("const-unused-success", &[UNUSED_CONSTANT]);
    assert_success(&project, "app.start", &[], "基础完成\n");
    assert_entry_failures(&project);
}

#[test]
fn project_used_constants_preserve_build_run_argv_and_entry_failure_output() {
    let project = Project::create("const-used-success", &[CONSTANT_ENTRY, USED_CONSTANTS]);
    assert_success(&project, "app.start", &[], "常量完成常量完成\n");
    assert_success(&project, "app.argv", &["实参"], "常量完成\n实参\n");
    assert_entry_failures(&project);
}

fn assert_gate(project: &Project, human: &str, json: &str) {
    // 同一错误必须挡住存在且合法、存在但非法、完全不存在的 entry。
    for entry in ["app.start", "app.invalid", "app.absent"] {
        for (format, stderr) in [("human", human), ("json", json)] {
            assert_failure(project, entry, format, stderr);
        }
    }
}

fn assert_entry_failures(project: &Project) {
    for (entry, stderr) in [
        ("app.invalid", INVALID_ENTRY),
        ("app.absent", MISSING_ENTRY),
    ] {
        for format in ["human", "json"] {
            assert_failure(project, entry, format, stderr);
        }
    }
}

fn assert_failure(project: &Project, entry: &str, format: &str, stderr: &str) {
    let executable = project.path().join("must-not-publish");
    let built = project.build(entry, format, &executable);
    assert_output(&built, 1, "", stderr);
    assert!(!executable.exists(), "failed build published an executable");
    project.assert_no_build_temporaries();

    let executed = project.run(entry, format, &[]);
    assert_output(&executed, 1, "", stderr);
    assert!(!executable.exists());
    project.assert_no_build_temporaries();
}

fn assert_success(project: &Project, entry: &str, arguments: &[&str], stdout: &str) {
    for format in ["human", "json"] {
        let executable = project.path().join(format!("{entry}-{format}"));
        let built = project.build(entry, format, &executable);
        assert_output(&built, 0, "", "");
        assert!(executable.is_file());
        project.assert_no_build_temporaries();

        let launched = Command::new(&executable)
            .args(arguments)
            .output()
            .expect("built oracle executable must launch");
        assert_output(&launched, 0, stdout, "");
        let executable_before = fs::read(&executable).expect("read built oracle executable");

        let executed = project.run(entry, format, arguments);
        assert_output(&executed, 0, stdout, "");
        assert_eq!(
            fs::read(&executable).expect("read preserved oracle executable"),
            executable_before,
            "run must preserve the previously published executable"
        );
        project.assert_no_build_temporaries();
    }
}

fn assert_output(output: &Output, status: i32, stdout: &str, stderr: &str) {
    assert_eq!(output.status.code(), Some(status), "{output:?}");
    assert_eq!(output.stdout, stdout.as_bytes(), "{output:?}");
    assert_eq!(output.stderr, stderr.as_bytes(), "{output:?}");
}

struct Project(PathBuf);

impl Project {
    fn create(label: &str, sources: &[Source]) -> Self {
        let project = Self(std::env::temp_dir().join(format!(
            "koven basic ownership cli {label}-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir(project.path()).expect("oracle directory must be creatable");
        // 声明/写入顺序故意与 source-key 排序相反。
        for root in ["zroot", "aroot"] {
            fs::create_dir(project.path().join(root)).expect("oracle root must be creatable");
        }
        project.write(
            "project.toml",
            "schema = \"koven.project\"\nversion = 1\n\n[project]\nname = \"ownership-oracle\"\nsource-roots = [\"zroot\", \"aroot\"]\n",
        );
        project.write(ENTRY.0, ENTRY.1);
        for &(path, text) in sources {
            project.write(path, text);
        }
        project
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("oracle source parent must be creatable");
        }
        fs::write(path, text).expect("oracle source must be writable");
    }

    fn command(&self, action: &str, entry: &str, format: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kovenc"));
        command
            .arg(format!("--message-format={format}"))
            .arg(action)
            .arg("--project")
            .arg(self.path().join("project.toml"))
            .arg("--entry")
            .arg(entry);
        command
    }

    fn build(&self, entry: &str, format: &str, executable: &Path) -> Output {
        self.command("build", entry, format)
            .arg("-o")
            .arg(executable)
            .output()
            .expect("kovenc build must launch")
    }

    fn run(&self, entry: &str, format: &str, arguments: &[&str]) -> Output {
        let mut command = self.command("run", entry, format);
        if !arguments.is_empty() {
            command.arg("--").args(arguments);
        }
        command.output().expect("kovenc run must launch")
    }

    fn assert_no_build_temporaries(&self) {
        assert!(
            fs::read_dir(self.path())
                .expect("oracle directory must be readable")
                .all(|entry| !entry
                    .expect("oracle directory entry must be readable")
                    .file_name()
                    .to_string_lossy()
                    .contains(".kovenc-")),
            "build must clean both temporary object and executable"
        );
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        fs::remove_dir_all(self.path()).expect("owned oracle directory must be removable");
    }
}
