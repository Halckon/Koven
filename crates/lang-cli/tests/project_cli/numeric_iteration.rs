//! SPEC-0329 project counterpart for numeric Range values and storage.
#[path = "../support/numeric_iteration_run.rs"]
mod bounded_run;

use super::{TestProject, assert_no_build_temporaries, project_build};

#[test]
fn project_numeric_range_iteration_executes_bounded_extremes_and_cleanup() {
    let project = TestProject::create("integer-range-iteration-0329");
    let manifest = project.manifest(&["src"]);
    project.write(
        "src/main.ko",
        include_str!("../fixtures/integer-range-iteration-0329/success.ko"),
    );
    let executable = project.join("range");
    let built = project_build(&manifest, "main", &executable, None);
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
    assert_no_build_temporaries(project.path());
}
