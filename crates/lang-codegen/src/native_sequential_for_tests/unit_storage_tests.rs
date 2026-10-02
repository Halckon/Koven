//! SPEC-0248: native Unit containers retain source order and logical cardinality.

use super::emit_link_and_run;

fn check(
    filename: &str,
    provider: &str,
    constructor: &str,
    arguments: &str,
    expected_count: usize,
    expected_stdout: &[u8],
) {
    let source = format!(
        r#"
        fun first(): Unit {{ println("first") }}
        fun second(): Unit {{ println("second") }}
        fun third(): Unit {{ println("third") }}
        fun source(): {provider}<Unit> {{
            println("source")
            return {constructor}<Unit>({arguments})
        }}
        fun main(): Unit {{
            var count = 0
            for (item in source()) {{
                count += 1
                val copy: Unit = item
                println("body")
            }}
            if (count == {expected_count}) {{ println("count-ok") }}
            println("done")
        }}
        "#,
    );
    eprintln!("{filename}:\n{source}");
    let run = emit_link_and_run(filename, &source, "main");
    assert!(run.status.success(), "{filename}: {run:?}");
    assert_eq!(run.stdout, expected_stdout, "{filename}: {run:?}");
    assert!(run.stderr.is_empty(), "{filename}: {run:?}");
}

#[test]
fn unit_array_empty_runs_natively() {
    check(
        "unit_array_empty.ko",
        "Array",
        "arrayOf",
        "",
        0,
        b"source\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_array_single_runs_natively() {
    check(
        "unit_array_single.ko",
        "Array",
        "arrayOf",
        "first()",
        1,
        b"source\nfirst\nbody\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_array_multi_runs_natively() {
    check(
        "unit_array_multi.ko",
        "Array",
        "arrayOf",
        "first(), second(), third()",
        3,
        b"source\nfirst\nsecond\nthird\nbody\nbody\nbody\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_list_empty_runs_natively() {
    check(
        "unit_list_empty.ko",
        "List",
        "listOf",
        "",
        0,
        b"source\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_list_single_runs_natively() {
    check(
        "unit_list_single.ko",
        "List",
        "listOf",
        "first()",
        1,
        b"source\nfirst\nbody\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_list_multi_runs_natively() {
    check(
        "unit_list_multi.ko",
        "List",
        "listOf",
        "first(), second(), third()",
        3,
        b"source\nfirst\nsecond\nthird\nbody\nbody\nbody\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_mutable_list_empty_runs_natively() {
    check(
        "unit_mutable_list_empty.ko",
        "MutableList",
        "mutableListOf",
        "",
        0,
        b"source\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_mutable_list_single_runs_natively() {
    check(
        "unit_mutable_list_single.ko",
        "MutableList",
        "mutableListOf",
        "first()",
        1,
        b"source\nfirst\nbody\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_mutable_list_multi_runs_natively() {
    check(
        "unit_mutable_list_multi.ko",
        "MutableList",
        "mutableListOf",
        "first(), second(), third()",
        3,
        b"source\nfirst\nsecond\nthird\nbody\nbody\nbody\ncount-ok\ndone\n",
    );
}

#[test]
fn unit_discard_binding_advances_logical_cursor_natively() {
    let source = r#"
        fun first(): Unit { println("first") }
        fun second(): Unit { println("second") }
        fun third(): Unit { println("third") }
        fun source(): List<Unit> {
            println("source")
            return listOf<Unit>(first(), second(), third())
        }
        fun main(): Unit {
            var count = 0
            for (_ in source()) {
                count += 1
                println("body")
            }
            if (count == 3) { println("count-ok") }
            println("done")
        }
    "#;
    let run = emit_link_and_run("unit_list_discard.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"source\nfirst\nsecond\nthird\nbody\nbody\nbody\ncount-ok\ndone\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn unit_named_binding_read_after_if_runs_natively() {
    let source = r#"
        fun first(): Unit { println("first") }
        fun main(): Unit {
            var count = 0
            for (item in arrayOf<Unit>(first())) {
                if (count == 0) { println("branch") }
                val copy: Unit = item
                count += 1
                println("body")
            }
            if (count == 1) { println("count-ok") }
        }
    "#;
    let run = emit_link_and_run("unit_array_read_after_if.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"first\nbranch\nbody\ncount-ok\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn unit_empty_mutable_list_constructor_runs_natively() {
    let source = r#"
        fun source(): MutableList<Unit> {
            println("source")
            return MutableList<Unit>()
        }
        fun main(): Unit {
            var count = 0
            for (item in source()) {
                count += 1
                val copy: Unit = item
                println("body")
            }
            if (count == 0) { println("count-ok") }
            println("done")
        }
    "#;
    let run = emit_link_and_run("unit_empty_mutable_list_constructor.ko", source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"source\ncount-ok\ndone\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn unit_value_return_retains_l0087_at_the_returned_expression() {
    let text = "fun unitCall(): Unit {}\nfun invalid(): Unit { return unitCall() }";
    let analysis = super::super::analyze("unit_invalid_return.ko", text);
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    let diagnostics = analysis.typed.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code().to_string(), "L0087");
    let start = text.rfind("unitCall()").unwrap();
    let expected = analysis
        .sources
        .span(
            analysis.parsed.source_id(),
            start,
            start + "unitCall()".len(),
        )
        .unwrap();
    assert_eq!(diagnostics[0].primary_span(), expected);
    assert_eq!(analysis.sources.slice(expected).unwrap(), "unitCall()");
}

#[test]
fn unit_source_element_replacement_retains_l0135_at_the_target() {
    let text = r#"
        fun unitCall(): Unit {}
        fun invalid(): Unit {
            var xs = arrayOf<Unit>(unitCall())
            for (item in xs) {
                val copy: Unit = item
                xs[0] = unitCall()
            }
        }
    "#;
    let analysis = super::super::analyze("unit_invalid_source_replacement.ko", text);
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    let diagnostics = analysis.owned.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code().to_string(), "L0135");
    let start = text.find("xs[0]").unwrap();
    let expected = analysis
        .sources
        .span(analysis.parsed.source_id(), start, start + "xs[0]".len())
        .unwrap();
    assert_eq!(diagnostics[0].primary_span(), expected);
    assert_eq!(analysis.sources.slice(expected).unwrap(), "xs[0]");
}
