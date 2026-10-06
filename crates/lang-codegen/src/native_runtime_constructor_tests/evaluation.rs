//! Normal source sequencing, including a language-level negative-length Abort.
use super::*;

pub(crate) struct EvaluationCase {
    pub(crate) label: String,
    // Single: format!("{}\n{}", case.api, case.entry).
    // Unit: package p + api; package q + imports + entry (real cross-file factory).
    pub(crate) api: String,
    pub(crate) entry: String,
    pub(crate) expected: &'static [u8],
    pub(crate) aborts: bool,
}

/// Six ordinary programs distinguish once-only operands and ordered invocations.
pub(crate) fn evaluation_cases() -> Vec<EvaluationCase> {
    let mut cases = Vec::new();
    for container in ["Array", "List"] {
        for length in [-1, 0, 3] {
            let api = format!(
                r#"fun size(): Int {{ println("size")
return {length} }}
fun trace(index: Int): Unit {{
    if (index == 0) {{ println("call-zero")
return }}
    if (index == 1) {{ println("call-one")
return }}
    if (index == 2) {{ println("call-two")
return }}
    error("unexpected callback index")
}}
fun factory(): (Int)->Int {{
    println("factory")
    return ({{ index -> trace(index)
index }})
}}"#
            );
            let check = if length == 3 {
                "if (items[2] != 2) { error(\"element\") }"
            } else {
                ""
            };
            let entry = format!(
                r#"fun entry(): Unit {{
    val items = {container}<Int>(size(), factory())
    if (items.size != {length}) {{ error("length") }}
    {check}
    println("after")
}}"#
            );
            cases.push(EvaluationCase {
                label: format!("{container}/length{length}"),
                api,
                entry,
                expected: match length {
                    -1 => b"size\n",
                    0 => b"size\nfactory\nafter\n",
                    3 => b"size\nfactory\ncall-zero\ncall-one\ncall-two\nafter\n",
                    _ => unreachable!("fixed fixture lengths"),
                },
                aborts: length < 0,
            });
        }
    }
    cases
}

/// Language-level Nothing operands abort before generation or any callback invocation.
pub(crate) fn nothing_operand_cases() -> Vec<EvaluationCase> {
    let mut cases = Vec::new();
    for container in ["Array", "List"] {
        for (label, operands) in [
            (
                "size",
                "error(\"size\"), { index -> println(\"call\")\nindex }",
            ),
            ("initializer", "3, error(\"initializer\")"),
        ] {
            cases.push(EvaluationCase {
                label: format!("{container}/Nothing-{label}"),
                api: "fun unused(): Unit {}".to_owned(),
                entry: format!("fun entry(): Unit {{ val unused = {container}<Int>({operands})\nprintln(\"after\") }}"),
                expected: b"",
                aborts: true,
            });
        }
    }
    cases
}

pub(crate) fn assert_evaluation_output(case: &EvaluationCase, output: &std::process::Output) {
    if case.aborts {
        assert!(!output.status.success(), "{}: {output:?}", case.label);
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(
                output.status.signal(),
                Some(6),
                "{}: {output:?}",
                case.label
            );
        }
        // Do not use a stderr-empty oracle for SIGABRT: platform crash reporting
        // is independent of whether size/factory/callback language effects ran.
    } else {
        assert!(output.status.success(), "{}: {output:?}", case.label);
        assert!(output.stderr.is_empty(), "{}: {output:?}", case.label);
    }
    assert_eq!(output.stdout, case.expected, "{}", case.label);
}

#[test]
fn runtime_constructor_native_single_evaluates_operands_once_in_order() {
    let cases = evaluation_cases();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let text = format!("{}\n{}", case.api, case.entry);
        let output = emit_link_and_run("runtime-evaluation.ko", &text, "entry");
        assert_evaluation_output(&case, &output);
    }
}

#[test]
fn runtime_constructor_native_single_nothing_operands_do_not_generate() {
    let cases = nothing_operand_cases();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let text = format!("{}\n{}", case.api, case.entry);
        let output = emit_link_and_run("runtime-nothing.ko", &text, "entry");
        assert_evaluation_output(&case, &output);
    }
}
