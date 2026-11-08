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
    super::assert_build_entry_count(&directory.0, &executable, 2);
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
    super::assert_build_entry_count(&directory.0, &executable, 2);
}

#[test]
fn single_if_retest_executes_both_branches_and_empty_input() {
    let directory = TestDirectory::create();
    let source = directory.join("while.ko");
    let executable = directory.join("while");
    fs::write(
        &source,
        include_str!("../fixtures/while-flow-0330/retest.ko"),
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
        include_bytes!("../fixtures/while-flow-0330/retest.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    super::assert_build_entry_count(&directory.0, &executable, 2);
}

#[test]
fn single_numeric_for_preserves_outer_proof_and_empty_input() {
    let directory = TestDirectory::create();
    let source = directory.join("while.ko");
    let executable = directory.join("while");
    fs::write(
        &source,
        include_str!("../fixtures/while-flow-0330/numeric.ko"),
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
        include_bytes!("../fixtures/while-flow-0330/numeric.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    super::assert_build_entry_count(&directory.0, &executable, 2);
}

#[test]
fn single_borrowed_for_preserves_outer_proof_and_empty_input() {
    let directory = TestDirectory::create();
    let source = directory.join("while.ko");
    let executable = directory.join("while");
    fs::write(
        &source,
        include_str!("../fixtures/while-flow-0330/inner-borrowed.ko"),
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
        include_bytes!("../fixtures/while-flow-0330/inner-borrowed.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    super::assert_build_entry_count(&directory.0, &executable, 2);
}

#[test]
fn single_nullable_while_preserves_outer_borrowed_for_access() {
    let directory = TestDirectory::create();
    let source = directory.join("while.ko");
    let executable = directory.join("while");
    fs::write(
        &source,
        include_str!("../fixtures/while-flow-0330/outer-borrowed.ko"),
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
        include_bytes!("../fixtures/while-flow-0330/outer-borrowed.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    super::assert_build_entry_count(&directory.0, &executable, 2);
}

#[test]
fn local_destructuring_routes_while_and_temporary_for_in_both_entries() {
    assert_both_native_entries(
        include_str!("../fixtures/while-flow-0330/local-destructuring.ko"),
        b"kept\nloop\nkept\nloop\nchanged\nfor-changed\nwhile-changed\n",
    );
}

#[test]
fn constant_while_preserves_condition_effects_and_terminating_operands_in_both_entries() {
    assert_both_native_entries(
        include_str!("../fixtures/while-flow-0330/constant-edges.ko"),
        b"kept\ncondition\nnormal exit\nprefix released\nprefix released\nfirst argument\nfirst argument\nafter break\n",
    );
}

fn assert_both_native_entries(source_text: &str, stdout: &[u8]) {
    let directory = TestDirectory::create();
    fs::create_dir(directory.join("src")).unwrap();
    let source = directory.join("src/main.ko");
    fs::write(&source, source_text).unwrap();
    let manifest = directory.join("project.toml");
    fs::write(
        &manifest,
        "schema = \"koven.project\"\nversion = 1\n[project]\nname = \"while-flow\"\nsource-roots = [\"src\"]\n",
    )
    .unwrap();

    for label in ["single", "project"] {
        let executable = directory.join(label);
        let built = if label == "single" {
            run([
                OsStr::new("build"),
                source.as_os_str(),
                OsStr::new("-o"),
                executable.as_os_str(),
            ])
        } else {
            run([
                OsStr::new("build"),
                OsStr::new("--project"),
                manifest.as_os_str(),
                OsStr::new("--entry"),
                OsStr::new("main"),
                OsStr::new("-o"),
                executable.as_os_str(),
            ])
        };
        assert_eq!(built.status.code(), Some(0), "{label}: {built:?}");
        assert!(
            built.stdout.is_empty() && built.stderr.is_empty(),
            "{built:?}"
        );

        let launched = bounded_run::run(&executable);
        assert_eq!(launched.status.code(), Some(0), "{label}: {launched:?}");
        assert_eq!(launched.stdout, stdout, "{label}");
        assert!(launched.stderr.is_empty(), "{launched:?}");
    }
}
