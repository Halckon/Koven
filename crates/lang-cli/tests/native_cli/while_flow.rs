//! SPEC-0330 ordinary while flow; bounded successful source only.
use super::bounded_iteration_run as bounded_run;

use super::{OsStr, TestDirectory, fs, run};

#[test]
fn single_while_flow_executes_bounded_success_and_cleanup() {
    let directory = TestDirectory::create();
    let source = directory.join("while.ko");
    let executable = directory.join("while");
    fs::write(
        &source,
        include_str!("../fixtures/while-flow-0330/success.ko"),
    )
    .unwrap();
    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    assert!(
        built.stdout.is_empty() && built.stderr.is_empty(),
        "{built:?}"
    );
    let launched = bounded_run::run(&executable);
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(
        launched.stdout,
        include_bytes!("../fixtures/while-flow-0330/success.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn single_pending_field_child_executes_both_branches_and_empty_input() {
    let directory = TestDirectory::create();
    let source = directory.join("while.ko");
    let executable = directory.join("while");
    fs::write(
        &source,
        include_str!("../fixtures/while-flow-0330/pending.ko"),
    )
    .unwrap();
    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    assert!(
        built.stdout.is_empty() && built.stderr.is_empty(),
        "{built:?}"
    );
    let launched = bounded_run::run(&executable);
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(
        launched.stdout,
        include_bytes!("../fixtures/while-flow-0330/pending.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}
