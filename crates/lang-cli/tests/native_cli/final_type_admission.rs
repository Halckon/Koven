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
    assert_rejections(&cases, 1);
    assert_numeric_range_probe();
}

fn assert_numeric_range_probe() {
    for command in ["build", "run"] {
        for format in ["human", "json"] {
            let directory = TestDirectory::create();
            let source = directory.join("p34_for_range.ko");
            let executable = directory.join("numeric-range");
            fs::write(&source, support::NUMERIC_RANGE_SOURCE).expect("original p34 bytes");
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
            eprintln!("p34 success {command} {format}: {output:?}");
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
            assert_eq!(
                output.stdout,
                if command == "run" {
                    b"for-range\n".as_slice()
                } else {
                    b"".as_slice()
                }
            );
            assert_eq!(executable.exists(), command == "build");
            let mut after = directory_snapshot(&directory.0);
            if command == "build" {
                super::build_outputs::assert_debug_bundle(&executable);
                after.retain(|(path, _)| !super::build_outputs::is_final_output(&executable, path));
            }
            assert_eq!(after, before, "source bytes and temporary output cleanup");
        }
    }
}

#[test]
fn single_deferred_user_message_build_preserves_source_root() {
    let deferred = support::deferred_call_probe();
    let mut cases = Vec::new();
    for (probe, message) in [
        (
            &deferred,
            "type checking could not complete: the call's type is unresolved",
        ),
        (
            &PROBES[2],
            "for source requires a compiler-bound sequential container, View, IntRange or LongRange",
        ),
    ] {
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
            expected["message"] = message.into();
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
    }
    support::assert_rejections_count(&cases, 1, 4);
}

// SPEC-0323: only invoke the compiler's build command, never a produced artifact.
#[test]
fn single_static_recovery_build_preserves_source_roots() {
    let mut cases = Vec::new();
    let mut companion_cases = Vec::new();
    for (index, probe) in support::static_recovery_probes().iter().enumerate() {
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
            eprintln!("SPEC-0341 {} build {format}: {output:?}", probe.name);
            if index == 0 {
                companion_cases.push((
                    output,
                    before,
                    directory_snapshot(&directory.0),
                    executable.exists(),
                ));
                continue;
            }
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
    assert_eq!(companion_cases.len(), 2, "original p28 renderer slots");
    assert_eq!(cases.len(), 2, "original p16 renderer slots");
    for (output, before, after, executable_exists) in companion_cases {
        assert!(!executable_exists, "unexpected companion executable");
        assert_eq!(after, before, "original source and directory bytes");
        support::assert_companion_capability_rejection(&output, 2, "native");
    }
    support::assert_rejections_count(&cases, 1, 2);
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
    super::build_outputs::assert_debug_bundle(&executable);
    let mut snapshot = directory_snapshot(&directory.0);
    snapshot.retain(|(path, _)| {
        path == std::path::Path::new(executable.file_name().expect("output name"))
            || !super::build_outputs::is_final_output(&executable, path)
    });
    assert_eq!(
        snapshot.len(),
        2,
        "source and executable only outside the debug bundle"
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
    support::assert_rejections_count(&cases, 1, 4);
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
    support::assert_rejections_count(&cases, 1, 2);
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
    super::build_outputs::assert_debug_bundle(&executable);
    let mut snapshot = directory_snapshot(&directory.0);
    snapshot.retain(|(path, _)| {
        path == std::path::Path::new(executable.file_name().expect("output name"))
            || !super::build_outputs::is_final_output(&executable, path)
    });
    assert_eq!(
        snapshot.len(),
        2,
        "source and executable only outside the debug bundle"
    );
}
