//! SPEC-0320: direct payloadless enum Borrow without a when expression.
use super::{Command, OsStr, TestDirectory, fs, run};

#[path = "../support/enum_case_borrow.rs"]
mod support;
use support::{BuildRunCase, SOURCE, assert_successes, remaining_temporaries};

#[test]
fn single_direct_case_borrow_builds_and_runs() {
    let directory = TestDirectory::create();
    let source = directory.join("enum-borrow.ko");
    let executable = directory.join("enum-borrow");
    fs::write(&source, SOURCE.as_bytes()).expect("write original fixture bytes");
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
    assert_successes(&[BuildRunCase {
        context: "single",
        built,
        executed,
        artifact,
        executable_exists,
        source_unchanged: fs::read(&source).expect("read original source") == SOURCE.as_bytes(),
        remaining_temporaries: remaining_temporaries(&directory.0),
    }]);
}
