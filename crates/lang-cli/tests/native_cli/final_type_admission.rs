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
