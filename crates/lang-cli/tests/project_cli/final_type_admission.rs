//! SPEC-0319 普通 project 与 constant fallback 共用最终类型准入。

use super::{OsStr, TestProject, project_build, run};

#[path = "../support/final_type_admission.rs"]
mod support;

use support::{PROBES, RejectionCase, assert_rejections, directory_snapshot, expected_diagnostic};

const CONSTANT_PREFIX: &str = "const val FINAL_TYPE_ADMISSION_MARKER: Int = 1\n";

#[test]
fn project_probe_roots_reject_before_codegen() {
    assert_project_probes("");
}

#[test]
fn project_const_fallback_keeps_type_admission() {
    // 与原始单一真源分开生成；这 47 个 UTF-8 字节把根因移至第二行。
    assert_eq!(CONSTANT_PREFIX.len(), 47);
    let shifted_spans = [(87, 93), (91, 97), (87, 92), (79, 80)];
    for (probe, (start, end)) in PROBES.iter().zip(shifted_spans) {
        let expected = expected_diagnostic(probe, probe.name, CONSTANT_PREFIX);
        assert_eq!(expected["primary"]["byte_start"], start);
        assert_eq!(expected["primary"]["byte_end"], end);
        assert_eq!(expected["primary"]["start"]["line"], 2);
        assert_eq!(expected["primary"]["end"]["line"], 2);
    }
    assert_project_probes(CONSTANT_PREFIX);
}

fn assert_project_probes(prefix: &str) {
    let mut cases = Vec::new();
    for probe in &PROBES {
        for command in ["build", "run"] {
            for format in ["human", "json"] {
                let context = format!(
                    "{} {command} {format} constants={}",
                    probe.name,
                    !prefix.is_empty()
                );
                let project = TestProject::create("final-type-admission");
                let manifest = project.manifest(&["src"]);
                let source_name = format!("src/{}", probe.name);
                project.write(&source_name, &format!("{prefix}{}", probe.text));
                let executable = project.join("rejected-program");
                let before = directory_snapshot(project.path());
                let format_option = format!("--message-format={format}");
                let output = if command == "build" {
                    project_build(&manifest, "main", &executable, Some(format))
                } else {
                    run([
                        OsStr::new(&format_option),
                        OsStr::new("run"),
                        OsStr::new("--project"),
                        manifest.as_os_str(),
                        OsStr::new("--entry"),
                        OsStr::new("main"),
                    ])
                };
                let expected = expected_diagnostic(probe, &source_name, prefix);
                cases.push(RejectionCase {
                    context,
                    output,
                    format,
                    expected,
                    before,
                    after: directory_snapshot(project.path()),
                    executable_exists: executable.exists(),
                });
            }
        }
    }
    assert_rejections(&cases, 1);
}

#[test]
fn project_deferred_user_message_build_preserves_source_root() {
    let probe = &PROBES[2];
    let mut cases = Vec::new();
    for format in ["human", "json"] {
        let project = TestProject::create("deferred-message-0324");
        let manifest = project.manifest(&["src"]);
        let source_name = format!("src/{}", probe.name);
        project.write(&source_name, probe.text);
        let executable = project.join("rejected-program");
        let before = directory_snapshot(project.path());
        let output = project_build(&manifest, "main", &executable, Some(format));
        let mut expected = expected_diagnostic(probe, &source_name, "");
        expected["message"] =
            "type checking could not complete: the call's type is unresolved".into();
        cases.push(RejectionCase {
            context: format!("SPEC-0324 project build {format}"),
            output,
            format,
            expected,
            before,
            after: directory_snapshot(project.path()),
            executable_exists: executable.exists(),
        });
    }
    support::assert_rejections_count(&cases, 1, 2);
}

#[test]
fn project_static_recovery_build_preserves_source_root() {
    assert_static_recovery_project_build("");
}

#[test]
fn project_constant_static_recovery_build_preserves_source_root() {
    assert_eq!(CONSTANT_PREFIX.len(), 47);
    assert_static_recovery_project_build(CONSTANT_PREFIX);
}

fn assert_static_recovery_project_build(prefix: &str) {
    // Companion unit typing is complete; its unrelated backend boundary is excluded.
    let probes = support::static_recovery_probes();
    let probe = &probes[1];
    let mut cases = Vec::new();
    for format in ["human", "json"] {
        let project = TestProject::create("static-recovery-0323");
        let manifest = project.manifest(&["src"]);
        let source_name = format!("src/{}", probe.name);
        project.write(&source_name, &format!("{prefix}{}", probe.text));
        let executable = project.join("rejected-program");
        let before = directory_snapshot(project.path());
        let output = project_build(&manifest, "main", &executable, Some(format));
        cases.push(RejectionCase {
            context: format!("SPEC-0323 build {format} constants={}", !prefix.is_empty()),
            output,
            format,
            expected: expected_diagnostic(probe, &source_name, prefix),
            before,
            after: directory_snapshot(project.path()),
            executable_exists: executable.exists(),
        });
    }
    support::assert_rejections_count(&cases, 1, 2);
}

#[test]
fn project_assignment_capture_build_preserves_ownership_rejection() {
    let prefix = "";
    let probe = &support::assignment_capture_probe();
    let mut cases = Vec::new();
    for format in ["human", "json"] {
        let project = TestProject::create("assignment-capture-0325");
        let manifest = project.manifest(&["src"]);
        let source_name = format!("src/{}", probe.name);
        project.write(&source_name, &format!("{prefix}{}", probe.text));
        let executable = project.join("rejected-program");
        let before = directory_snapshot(project.path());
        let output = project_build(&manifest, "main", &executable, Some(format));
        cases.push(RejectionCase {
            context: format!("SPEC-0325 build {format}"),
            output,
            format,
            expected: expected_diagnostic(probe, &source_name, prefix),
            before,
            after: directory_snapshot(project.path()),
            executable_exists: executable.exists(),
        });
    }
    support::assert_rejections_count(&cases, 1, 2);
}
