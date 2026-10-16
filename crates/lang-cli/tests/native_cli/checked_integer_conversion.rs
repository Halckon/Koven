//! SPEC-0327 ordinary successful conversions; no out-of-range/Abort cases.
use super::{OsStr, TestDirectory, fs, run};

#[test]
fn single_all_integer_conversion_boundaries_execute() {
    let directory = TestDirectory::create();
    let source = directory.join("conversion.ko");
    let executable = directory.join("conversion");
    fs::write(
        &source,
        include_str!("../fixtures/checked-integer-conversion-0327/success.ko"),
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
        include_bytes!("../fixtures/checked-integer-conversion-0327/success.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}
