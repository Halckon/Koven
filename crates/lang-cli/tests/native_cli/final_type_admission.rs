//! SPEC-0319 普通 single build/run 必须先给唯一源级根因。

use super::{OsStr, TestDirectory, fs, run};

#[path = "../support/final_type_admission.rs"]
mod support;

use support::{PROBES, RejectionCase, assert_rejections, directory_snapshot, expected_diagnostic};

#[test]
fn single_probe_roots_reject_before_codegen() {
    let mut cases = Vec::new();
    for probe in &PROBES {
        for command in ["build", "run"] {
            for format in ["human", "json"] {
                let context = format!("{} {command} {format}", probe.name);
                let directory = TestDirectory::create();
                let source = directory.join(probe.name);
                let executable = directory.join("rejected-program");
                fs::write(&source, probe.text.as_bytes()).expect("unchanged probe fixture write");
                let before = directory_snapshot(&directory.0);
                let format_option = format!("--message-format={format}");
                let output = if command == "build" {
                    run([
                        OsStr::new(&format_option),
                        OsStr::new("build"),
                        source.as_os_str(),
                        OsStr::new("-o"),
                        executable.as_os_str(),
                    ])
                } else {
                    run([
                        OsStr::new(&format_option),
                        OsStr::new("run"),
                        source.as_os_str(),
                    ])
                };
                let expected = expected_diagnostic(
                    probe,
                    source.to_str().expect("UTF-8 single-file source path"),
                    "",
                );
                cases.push(RejectionCase {
                    context,
                    output,
                    format,
                    expected,
                    before,
                    after: directory_snapshot(&directory.0),
                    executable_exists: executable.exists(),
                });
            }
        }
    }
    assert_rejections(&cases, 2);
}

#[test]
fn single_deferred_user_message_build_preserves_source_root() {
    let probe = &PROBES[2];
    let mut cases = Vec::new();
    for format in ["human", "json"] {
        let directory = TestDirectory::create();
        let source = directory.join(probe.name);
        let executable = directory.join("rejected-program");
        fs::write(&source, probe.text.as_bytes()).expect("original fixture write");
        let before = directory_snapshot(&directory.0);
        let format_option = format!("--message-format={format}");
        let output = run([
            OsStr::new(&format_option),
            OsStr::new("build"),
            source.as_os_str(),
            OsStr::new("-o"),
            executable.as_os_str(),
        ]);
        let mut expected = expected_diagnostic(probe, source.to_str().unwrap(), "");
        expected["message"] =
            "type checking could not complete: the call's type is unresolved".into();
        cases.push(RejectionCase {
            context: format!("SPEC-0324 single build {format}"),
            output,
            format,
            expected,
            before,
            after: directory_snapshot(&directory.0),
            executable_exists: executable.exists(),
        });
    }
    support::assert_rejections_count(&cases, 2, 2);
}

// SPEC-0323: only invoke the compiler's build command, never a produced artifact.
#[test]
fn single_static_recovery_build_preserves_source_roots() {
    let mut cases = Vec::new();
    for probe in &support::static_recovery_probes() {
        for format in ["human", "json"] {
            let directory = TestDirectory::create();
            let source = directory.join(probe.name);
            let executable = directory.join("rejected-program");
            fs::write(&source, probe.text.as_bytes()).expect("original fixture write");
            let before = directory_snapshot(&directory.0);
            let format_option = format!("--message-format={format}");
            let output = run([
                OsStr::new(&format_option),
                OsStr::new("build"),
                source.as_os_str(),
                OsStr::new("-o"),
                executable.as_os_str(),
            ]);
            cases.push(RejectionCase {
                context: format!("SPEC-0323 {} build {format}", probe.name),
                output,
                format,
                expected: expected_diagnostic(probe, source.to_str().unwrap(), ""),
                before,
                after: directory_snapshot(&directory.0),
                executable_exists: executable.exists(),
            });
        }
    }
    support::assert_rejections_count(&cases, 2, 4);
}

#[test]
fn single_integer_control_joins_execute() {
    let directory = TestDirectory::create();
    let source = directory.join("local_unit.ko");
    let executable = directory.join("local-unit");
    fs::write(
        &source,
        include_str!("../fixtures/control-join-0326/integer_joins.ko"),
    )
    .expect("ordinary fixture write");
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
    let launched = std::process::Command::new(&executable)
        .output()
        .expect("ordinary executable");
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(launched.stdout, b"joined\n");
    assert!(launched.stderr.is_empty(), "{launched:?}");
    assert_eq!(
        directory_snapshot(&directory.0).len(),
        2,
        "source and executable only"
    );
}

// SPEC-0326: only invoke the compiler's build command, never a produced artifact.
#[test]
fn single_mixed_control_join_build_preserves_source_rejection() {
    let mut cases = Vec::new();
    for probe in &support::control_join_probes() {
        for format in ["human", "json"] {
            let directory = TestDirectory::create();
            let source = directory.join(probe.name);
            let executable = directory.join("rejected-program");
            fs::write(&source, probe.text.as_bytes()).expect("original fixture write");
            let before = directory_snapshot(&directory.0);
            let format_option = format!("--message-format={format}");
            let output = run([
                OsStr::new(&format_option),
                OsStr::new("build"),
                source.as_os_str(),
                OsStr::new("-o"),
                executable.as_os_str(),
            ]);
            cases.push(RejectionCase {
                context: format!("SPEC-0326 {} build {format}", probe.name),
                output,
                format,
                expected: expected_diagnostic(probe, source.to_str().unwrap(), ""),
                before,
                after: directory_snapshot(&directory.0),
                executable_exists: executable.exists(),
            });
        }
    }
    support::assert_rejections_count(&cases, 2, 4);
}

// SPEC-0325: only invoke the compiler's build command, never a produced artifact.
#[test]
fn single_assignment_capture_build_preserves_ownership_rejection() {
    let mut cases = Vec::new();
    for probe in &[support::assignment_capture_probe()] {
        for format in ["human", "json"] {
            let directory = TestDirectory::create();
            let source = directory.join(probe.name);
            let executable = directory.join("rejected-program");
            fs::write(&source, probe.text.as_bytes()).expect("original fixture write");
            let before = directory_snapshot(&directory.0);
            let format_option = format!("--message-format={format}");
            let output = run([
                OsStr::new(&format_option),
                OsStr::new("build"),
                source.as_os_str(),
                OsStr::new("-o"),
                executable.as_os_str(),
            ]);
            cases.push(RejectionCase {
                context: format!("SPEC-0325 {} build {format}", probe.name),
                output,
                format,
                expected: expected_diagnostic(probe, source.to_str().unwrap(), ""),
                before,
                after: directory_snapshot(&directory.0),
                executable_exists: executable.exists(),
            });
        }
    }
    support::assert_rejections_count(&cases, 2, 2);
}

#[test]
fn single_local_assignment_unit_value_executes() {
    let directory = TestDirectory::create();
    let source = directory.join("local_unit.ko");
    let executable = directory.join("local-unit");
    fs::write(
        &source,
        include_str!("../fixtures/simple-assignment-0325/local_unit.ko"),
    )
    .expect("ordinary fixture write");
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
    let launched = std::process::Command::new(&executable)
        .output()
        .expect("ordinary executable");
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(launched.stdout, b"assigned\n");
    assert!(launched.stderr.is_empty(), "{launched:?}");
    assert_eq!(
        directory_snapshot(&directory.0).len(),
        2,
        "source and executable only"
    );
}
