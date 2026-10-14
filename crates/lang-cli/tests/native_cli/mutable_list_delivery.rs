//! SPEC-0321：原 p38 的单文件公开 build/run 正例。

use super::{Command, OsStr, TestDirectory, fs, run};

#[path = "../support/mutable_list_delivery.rs"]
mod support;
use support::{BuildRunCase, P38_SOURCE, assert_success, remaining_temporaries};

#[test]
fn single_p38_string_add_then_for_builds_and_runs() {
    let directory = TestDirectory::create();
    let source = directory.join("p38_list_of_strings.ko");
    let executable = directory.join("list-of-strings");
    fs::write(&source, P38_SOURCE.as_bytes()).expect("write original p38 fixture bytes");
    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    let executed = run([OsStr::new("run"), source.as_os_str()]);
    let executable_exists = executable.is_file();
    let artifact = (built.status.success() && executable_exists).then(|| {
        Command::new(&executable)
            .output()
            .expect("ordinary built artifact starts")
    });
    assert_success(BuildRunCase {
        context: "p38 single",
        built,
        executed,
        artifact,
        executable_exists,
        source_unchanged: fs::read(&source).expect("read original p38 source")
            == P38_SOURCE.as_bytes(),
        remaining_temporaries: remaining_temporaries(&directory.0),
    });
}
