//! SPEC-0328 project counterpart for numeric Range values and storage.
use super::{TestProject, assert_no_build_temporaries, project_build};

#[test]
fn project_integer_range_values_and_storage_execute() {
    let project = TestProject::create("integer-range-0328");
    let manifest = project.manifest(&["src"]);
    project.write(
        "src/main.ko",
        include_str!("../fixtures/integer-range-0328/success.ko"),
    );
    let executable = project.join("range");
    let built = project_build(&manifest, "main", &executable, None);
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
    assert_no_build_temporaries(project.path());
}
