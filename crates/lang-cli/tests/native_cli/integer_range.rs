//! SPEC-0328 ordinary numeric Range values; bounded successful source only.
use super::{OsStr, TestDirectory, fs, run};

#[test]
fn single_integer_range_values_and_storage_execute() {
    let directory = TestDirectory::create();
    let source = directory.join("range.ko");
    let executable = directory.join("range");
    fs::write(
        &source,
        include_str!("../fixtures/integer-range-0328/success.ko"),
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
    let launched = std::process::Command::new(&executable).output().unwrap();
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(
        launched.stdout,
        include_bytes!("../fixtures/integer-range-0328/success.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    super::assert_build_entry_count(&directory.0, &executable, 2);
}
