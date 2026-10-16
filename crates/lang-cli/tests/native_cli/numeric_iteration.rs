//! SPEC-0329 ordinary numeric Range values; bounded successful source only.
#[path = "../support/numeric_iteration_run.rs"]
mod bounded_run;

use super::{OsStr, TestDirectory, fs, run};

#[test]
fn single_numeric_range_iteration_executes_bounded_extremes_and_cleanup() {
    let directory = TestDirectory::create();
    let source = directory.join("range.ko");
    let executable = directory.join("range");
    fs::write(
        &source,
        include_str!("../fixtures/integer-range-iteration-0329/success.ko"),
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
        include_bytes!("../fixtures/integer-range-iteration-0329/success.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}
