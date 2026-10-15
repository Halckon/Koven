//! SPEC-0319 两个公开 CLI 入口共享原字节 fixture 和诊断 oracle。

use std::{
    fs,
    ops::Range,
    path::{Path, PathBuf},
    process::Output,
};

use serde_json::{Value, json};

pub(super) struct Probe {
    pub(super) name: &'static str,
    pub(super) text: &'static str,
    code: &'static str,
    primary: Range<usize>,
    token: &'static str,
}

pub(super) const PROBES: [Probe; 4] = [
    Probe {
        name: "r01_bogus_member.ko",
        text: include_str!(
            "../../../lang-frontend/tests/fixtures/final-type-admission-0319/r01_bogus_member.ko"
        ),
        code: "L0080",
        primary: 40..46,
        token: "fooBar",
    },
    Probe {
        name: "r02_bogus_member_str.ko",
        text: include_str!(
            "../../../lang-frontend/tests/fixtures/final-type-admission-0319/r02_bogus_member_str.ko"
        ),
        code: "L0080",
        primary: 44..50,
        token: "length",
    },
    Probe {
        name: "p34_for_range.ko",
        text: include_str!(
            "../../../lang-frontend/tests/fixtures/final-type-admission-0319/p34_for_range.ko"
        ),
        code: "L0165",
        primary: 40..45,
        token: "0..<5",
    },
    Probe {
        name: "s04_question_on_nullable.ko",
        text: include_str!(
            "../../../lang-frontend/tests/fixtures/final-type-admission-0319/s04_question_on_nullable.ko"
        ),
        code: "L0085",
        primary: 32..33,
        token: "?",
    },
];

// SPEC-0323: original ordinary-source probes, shared by build-only checks.
pub(super) fn static_recovery_probes() -> [Probe; 2] {
    [
        Probe {
            name: "p28_companion.ko",
            text: include_str!(
                "../../../lang-frontend/tests/fixtures/final-admission-static-recovery-0323/p28_companion.ko"
            ),
            code: "L0165",
            primary: 95..103,
            token: "P.origin",
        },
        Probe {
            name: "p16_result_q.ko",
            text: include_str!(
                "../../../lang-frontend/tests/fixtures/final-admission-static-recovery-0323/p16_result_q.ko"
            ),
            code: "L0165",
            primary: 185..194,
            token: "parse(n)?",
        },
    ]
}

pub(super) fn expected_diagnostic(probe: &Probe, source_name: &str, prefix: &str) -> Value {
    assert_eq!(&probe.text[probe.primary.clone()], probe.token);
    let text = format!("{prefix}{}", probe.text);
    let start = prefix.len() + probe.primary.start;
    let end = prefix.len() + probe.primary.end;
    assert_eq!(&text[start..end], probe.token);
    let (start_line, start_column) = line_column(&text, start);
    let (end_line, end_column) = line_column(&text, end);
    json!({
        "code": probe.code,
        "primary": {
            "source": source_name,
            "byte_start": start,
            "byte_end": end,
            "start": { "line": start_line, "column": start_column },
            "end": { "line": end_line, "column": end_column },
        },
    })
}

fn line_column(text: &str, byte_offset: usize) -> (usize, usize) {
    let before = &text[..byte_offset];
    let line = before.bytes().filter(|&byte| byte == b'\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
    (line, column)
}

pub(super) struct RejectionCase {
    pub(super) context: String,
    pub(super) output: Output,
    pub(super) format: &'static str,
    pub(super) expected: Value,
    pub(super) before: Vec<(PathBuf, Option<Vec<u8>>)>,
    pub(super) after: Vec<(PathBuf, Option<Vec<u8>>)>,
    pub(super) executable_exists: bool,
}

pub(super) fn assert_rejections(cases: &[RejectionCase], exit_code: i32) {
    assert_rejections_count(cases, exit_code, 16);
}

pub(super) fn assert_rejections_count(
    cases: &[RejectionCase],
    exit_code: i32,
    expected_count: usize,
) {
    // 先执行完矩阵并记录全部真实进程结果；首个红断言不能隐藏其余根因。
    for case in cases {
        eprintln!(
            "final-admission {} status={:?}\nstdout: {:?}\nstderr: {:?}",
            case.context,
            case.output.status.code(),
            String::from_utf8_lossy(&case.output.stdout),
            String::from_utf8_lossy(&case.output.stderr),
        );
    }
    assert_eq!(cases.len(), expected_count, "exact planned process count");
    for case in cases {
        assert!(
            !case.executable_exists,
            "{}: unexpected executable",
            case.context
        );
        assert_eq!(case.after, case.before, "{}", case.context);
        assert_rejection(
            &case.output,
            exit_code,
            case.format,
            &case.expected,
            &case.context,
        );
    }
}

fn assert_rejection(
    output: &Output,
    exit_code: i32,
    format: &str,
    expected: &Value,
    context: &str,
) {
    assert_eq!(
        output.status.code(),
        Some(exit_code),
        "{context}: {output:?}"
    );
    assert!(output.stdout.is_empty(), "{context}: {output:?}");
    let text = std::str::from_utf8(&output.stderr).expect("UTF-8 source diagnostics");
    assert!(!text.trim().is_empty(), "{context}: empty diagnostics");
    assert!(text.ends_with('\n'), "{context}: {text:?}");
    let lowercase = text.to_lowercase();
    for leaked in [
        "internal",
        "内部",
        "ownership",
        "blockingdeferred",
        "missingfact",
        "unsupportednode",
        "invalidssa",
        "codegen",
        "backend",
        "llvm",
        "linker",
        "panicked",
        "frontenddiagnostics",
    ] {
        assert!(
            !lowercase.contains(leaked),
            "{context}: leaked {leaked}: {text}"
        );
    }

    if format == "json" {
        let records = text
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("schema v1 JSON diagnostic"))
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 1, "{context}: {text}");
        let record = &records[0];
        assert_eq!(record["schema"], "koven.diagnostic", "{context}: {text}");
        assert_eq!(record["version"], 1, "{context}: {text}");
        assert_eq!(record["severity"], "error", "{context}: {text}");
        assert!(
            record["message"]
                .as_str()
                .is_some_and(|message| !message.trim().is_empty()),
            "{context}: {text}"
        );
        assert!(record["details"].is_array(), "{context}: {text}");
        assert_eq!(
            json!({ "code": record["code"], "primary": record["primary"] }),
            *expected,
            "{context}: {text}"
        );
    } else {
        assert_eq!(format, "human");
        let headers = text
            .lines()
            .filter(|line| !line.starts_with("  "))
            .collect::<Vec<_>>();
        assert_eq!(headers.len(), 1, "{context}: {text}");
        let primary = &expected["primary"];
        let source = primary["source"].as_str().expect("expected source");
        let source = source
            .replace('\\', "\\\\")
            .replace('\r', "\\r")
            .replace('\n', "\\n");
        let prefix = format!(
            "error[{}] {source}:{}:{}-{}:{}: ",
            expected["code"].as_str().expect("expected code"),
            primary["start"]["line"],
            primary["start"]["column"],
            primary["end"]["line"],
            primary["end"]["column"],
        );
        let message = headers[0]
            .strip_prefix(&prefix)
            .unwrap_or_else(|| panic!("{context}: wrong source diagnostic: {text}"));
        assert!(!message.trim().is_empty(), "{context}: {text}");
        assert!(
            text.lines().skip(1).all(|line| {
                line.starts_with("  label ")
                    || line.starts_with("  note: ")
                    || line.starts_with("  help: ")
            }),
            "{context}: {text}"
        );
    }
}

// 只检查本 case 拥有的目录，不扫描共享临时根以免与并行测试互相污染。
// 同时记录目录和文件字节：既不能新增产物，也不能改写原 fixture/manifest。
pub(super) fn directory_snapshot(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    fn visit(root: &Path, directory: &Path, entries: &mut Vec<(PathBuf, Option<Vec<u8>>)>) {
        for entry in fs::read_dir(directory).expect("owned fixture directory read") {
            let entry = entry.expect("owned fixture entry read");
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if entry.file_type().expect("fixture entry type").is_dir() {
                entries.push((relative, None));
                visit(root, &path, entries);
            } else {
                entries.push((relative, Some(fs::read(&path).expect("fixture file read"))));
            }
        }
    }
    let mut entries = Vec::new();
    visit(root, root, &mut entries);
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}
