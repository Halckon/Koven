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
