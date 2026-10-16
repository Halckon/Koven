//! SPEC-0330 project counterpart for bounded while flow and cleanup.
use super::bounded_iteration_run as bounded_run;

use super::{TestProject, assert_no_build_temporaries, project_build};

#[test]
fn project_while_flow_executes_bounded_success_and_cleanup() {
    let project = TestProject::create("while-flow-0330");
    let manifest = project.manifest(&["src"]);
    project.write(
        "src/main.ko",
        include_str!("../fixtures/while-flow-0330/success.ko"),
    );
    let executable = project.join("while");
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
        include_bytes!("../fixtures/while-flow-0330/success.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    assert_no_build_temporaries(project.path());
}
