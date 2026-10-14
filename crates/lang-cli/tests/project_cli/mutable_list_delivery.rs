//! SPEC-0321：basic/constant 独立观测，主源码保持原 p38 字节。

use super::{Command, OsStr, TestProject, fs, project_build, run};

#[path = "../support/mutable_list_delivery.rs"]
mod support;
use support::{BuildRunCase, P38_SOURCE, assert_success, remaining_temporaries};

const CONSTANT_SOURCE: &str = "const val MUTABLE_LIST_DELIVERY_MARKER: Int = 1\n";

#[test]
fn project_basic_p38_string_add_then_for_builds_and_runs() {
    assert_project_p38("p38 project basic", false);
}

#[test]
fn project_constant_p38_string_add_then_for_builds_and_runs() {
    assert_project_p38("p38 project constant", true);
}

fn assert_project_p38(context: &'static str, constants: bool) {
    let project = TestProject::create("mutable-list-delivery");
    let manifest = project.manifest(&["src"]);
    project.write("src/main.ko", P38_SOURCE);
    // 独立常量文件只选择 constant 路径，不改动原 p38 源码或 Span。
    if constants {
        project.write("src/constants.ko", CONSTANT_SOURCE);
    }
    let source = project.join("src/main.ko");
    let executable = project.join("list-of-strings");
    let built = project_build(&manifest, "main", &executable, None);
    let executed = run([
        OsStr::new("run"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("main"),
    ]);
    let executable_exists = executable.is_file();
    let artifact = (built.status.success() && executable_exists).then(|| {
        Command::new(&executable)
            .output()
            .expect("ordinary built artifact starts")
    });
    let source_unchanged = fs::read(&source).expect("read original p38 source")
        == P38_SOURCE.as_bytes()
        && (!constants
            || fs::read(project.join("src/constants.ko")).expect("read constant source")
                == CONSTANT_SOURCE.as_bytes());
    assert_success(BuildRunCase {
        context,
        built,
        executed,
        artifact,
        executable_exists,
        source_unchanged,
        remaining_temporaries: remaining_temporaries(project.path()),
    });
}
