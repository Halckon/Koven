//! SPEC-0327 project counterpart using the same explicit successful boundary fixture.
use super::{TestProject, assert_no_build_temporaries, project_build};

#[test]
fn project_all_integer_conversion_boundaries_execute() {
    let project = TestProject::create("checked-conversion-0327");
    let manifest = project.manifest(&["src"]);
    project.write(
        "src/main.ko",
        include_str!("../fixtures/checked-integer-conversion-0327/success.ko"),
    );
    let executable = project.join("conversion");
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
        include_bytes!("../fixtures/checked-integer-conversion-0327/success.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    assert_no_build_temporaries(project.path());
}

#[path = "../support/checked_integer_conversion_abort.rs"]
mod abort_support;

// CI execution only; do not include in local successful-native exact manifests.
#[test]
fn project_out_of_range_integer_conversions_abort() {
    for (name, source_type, target_type, argument) in abort_support::CASES {
        let project = TestProject::create("checked-conversion-abort-0327");
        let manifest = project.manifest(&["src"]);
        project.write(
            "src/main.ko",
            &abort_support::source(source_type, target_type, argument),
        );
        let executable = project.join("conversion");
        let built = project_build(&manifest, "main", &executable, None);
        assert_eq!(built.status.code(), Some(0), "{name}: {built:?}");
        assert!(
            built.stdout.is_empty() && built.stderr.is_empty(),
            "{name}: {built:?}"
        );
        abort_support::assert_abort(&executable, project.path());
        assert_no_build_temporaries(project.path());
    }
}
