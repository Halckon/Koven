//! SPEC-0291: unmodified source values and normal native exit after consume.
//! This does not observe insertion links, allocation identity or drop counts.

use super::resource_exchange_tests::run_unit;
use crate::native_tests::emit_link_and_run;

const SOURCE: &str = include_str!("../../tests/fixtures/map-consume-0291/owned-locals.ko");
const EXPECTED: &[u8] = include_bytes!("../../tests/fixtures/map-consume-0291/owned-locals.stdout");

#[test]
fn single_map_consume_preserves_values_and_normal_exit() {
    let run = emit_link_and_run("map-consume-owned-locals.ko", SOURCE, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, EXPECTED);
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn unit_map_consume_preserves_values_and_normal_exit() {
    let provider = format!("package p\n{SOURCE}");
    let consumer = "package q\nfun entry(): Unit { p.main() }";
    let (run, _) = run_unit(&provider, consumer, false);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, EXPECTED);
    assert!(run.stderr.is_empty(), "{run:?}");
}

// Nullable String value fixture; bind only after its two-entry complete LLVM gate passes.
const NULLABLE_STRING_SOURCE: &str =
    include_str!("../../tests/fixtures/map-consume-0291/nullable-strings.ko");
const NULLABLE_STRING_EXPECTED: &[u8] =
    include_bytes!("../../tests/fixtures/map-consume-0291/nullable-strings.stdout");

#[test]
fn single_nullable_map_consume_preserves_strings_and_normal_exit() {
    let run = emit_link_and_run(
        "nullable-map-consume-strings.ko",
        NULLABLE_STRING_SOURCE,
        "main",
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, NULLABLE_STRING_EXPECTED);
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn unit_nullable_map_consume_preserves_strings_and_normal_exit() {
    let provider = format!("package p\n{NULLABLE_STRING_SOURCE}");
    let consumer = "package q\nfun entry(): Unit { p.main() }\n";
    let (run, _) = run_unit(&provider, consumer, false);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, NULLABLE_STRING_EXPECTED);
    assert!(run.stderr.is_empty(), "{run:?}");
}

// Exact ordinary C1/C2/C3 sources; C3 keeps its original present-then-absent calls.
const NULLABLE_C_EXPECTED: [(&str, &[u8]); 3] = [
    ("C1", b"consumed empty\n"),
    ("C2", b"consumed strings\n"),
    ("C3", b"input consumed\n"),
];

#[test]
fn single_nullable_map_consume_original_cases_preserve_normal_exit() {
    let observations = crate::ssa::ordinary_nullable_consume_sources()
        .into_iter()
        .map(|(id, path, single, _, _)| (id, emit_link_and_run(path, single, "main")))
        .collect();
    assert_nullable_c_outputs(observations);
}

#[test]
fn unit_nullable_map_consume_original_cases_preserve_normal_exit() {
    let observations = crate::ssa::ordinary_nullable_consume_sources()
        .into_iter()
        .map(|(id, _, _, provider, consumer)| {
            let (run, _) = run_unit(provider, consumer, false);
            (id, run)
        })
        .collect();
    assert_nullable_c_outputs(observations);
}

fn assert_nullable_c_outputs(observations: Vec<(&str, std::process::Output)>) {
    // All three ordinary runs have returned before stdout/exit assertions begin.
    assert_eq!(observations.len(), NULLABLE_C_EXPECTED.len());
    for ((id, run), (expected_id, expected)) in observations.into_iter().zip(NULLABLE_C_EXPECTED) {
        assert_eq!(id, expected_id);
        assert!(run.status.success(), "{id}: {run:?}");
        assert_eq!(run.stdout, expected, "{id}");
        assert!(run.stderr.is_empty(), "{id}: {run:?}");
    }
}

pub(crate) mod temporary_null_cleanup_tests {
    //! SPEC-0296 phase two: temporary owners end after presence, before branch bodies.
    use super::super::owned_null_cleanup_tests::{run_single, run_units};

    fn comparison_forms() -> (String, String) {
        let mut source =
        "value class Text(val text: String)\nclass Resource { deinit() { println(\"drop\") } }\n"
            .to_owned();
        let mut calls = String::new();
        let mut expected = String::new();
        for (index, condition) in [
            "$ != null",
            "null != $",
            "(($)) != null",
            "$ == null",
            "null == (($))",
            "(($)) == null",
        ]
        .iter()
        .enumerate()
        {
            for (label, ty, value) in [
                ("string", "String", "\"text\".clone()"),
                ("inline", "Text", "Text(\"text\".clone())"),
                ("resource", "Resource", "Resource()"),
            ] {
                let condition = condition.replace('$', "m.remove(1)");
                source.push_str(&format!("fun {label}{index}(): Unit {{\n var m = mutableMapOf<Int, {ty}>()\n m.put(1, {value})\n val first = {condition}\n if (first) {{ println(\"yes\") }} else {{ println(\"no\") }}\n if ({condition}) {{ println(\"yes\") }} else {{ println(\"no\") }}\n}}\n"));
                calls.push_str(&format!("{label}{index}(); "));
                if label == "resource" {
                    expected.push_str("drop\n");
                }
                expected.push_str(if index < 3 { "yes\nno\n" } else { "no\nyes\n" });
            }
        }
        source.push_str(&format!("fun exercise(): Unit {{ {calls} }}"));
        (source, expected)
    }

    #[test]
    fn single_temporary_null_cleanup_scalar_and_if_matrix() {
        let (source, expected) = comparison_forms();
        run_single(&source, "exercise()", &expected);
    }

    #[test]
    fn unit_temporary_null_cleanup_scalar_and_if_matrix() {
        let (source, expected) = comparison_forms();
        run_units(&source, "p.exercise()", &expected);
    }

    const RESOURCE_TIMING: &str = r#"
class Resource(val text: String) { deinit() { println(this.text) } }
fun exercise(): Unit {
    var m = mutableMapOf<Int, Resource>()
    m.put(1, Resource("temporary drop"))
    if (((m.remove(1))) != null) { println("temporary body") }
    if (null == ((m.remove(1)))) { println("missing body") }
    m.put(2, Resource("named drop"))
    val value = m.remove(2)
    if (value != null) { println("named body") }
    println("before scope exit")
}
"#;

    #[test]
    fn single_temporary_null_cleanup_resource_precedes_body_named_stays_lexical() {
        run_single(
            RESOURCE_TIMING,
            "exercise()",
            "temporary drop\ntemporary body\nmissing body\nnamed body\nbefore scope exit\nnamed drop\n",
        );
    }

    #[test]
    fn unit_temporary_null_cleanup_resource_precedes_body_named_stays_lexical() {
        run_units(
            RESOURCE_TIMING,
            "p.exercise()",
            "temporary drop\ntemporary body\nmissing body\nnamed body\nbefore scope exit\nnamed drop\n",
        );
    }

    #[test]
    fn unit_temporary_null_cleanup_loop_exits() {
        for (exit, expected) in [
            (
                "",
                "one\npresent\nbody\ntail\nbody\ntail\ntwo\npresent\nbody\ntail\nafter\ndone\n",
            ),
            (
                "continue",
                "one\npresent\nbody\nbody\ntwo\npresent\nbody\nafter\ndone\n",
            ),
            ("break", "one\npresent\nbody\nafter\ntwo\ndone\n"),
            ("return", "one\npresent\nbody\ntwo\ndone\n"),
        ] {
            let source = format!(
                r#"
class Resource(val text: String) {{ deinit() {{ println(this.text) }} }}
fun exercise(args: Array<Int>): Unit {{
    var resources = mutableMapOf<Int, Resource>()
    resources.put(1, Resource("one"))
    resources.put(2, Resource("two"))
    for (key in args) {{
        if (((resources.remove(key))) != null) {{ println("present") }}
        println("body")
        {exit}
        println("tail")
    }}
    println("after")
}}
"#
            );
            run_units(
                &source,
                "p.exercise(arrayOf(1, 1, 2)); println(\"done\")",
                expected,
            );
        }
    }

    #[test]
    fn unit_temporary_null_cleanup_zero_one_and_repeated_string_loops() {
        let source = r#"
fun exercise(args: Array<String>): Unit {
    var texts = mutableMapOf<String, String>()
    for (word in args) { texts.put(word.clone(), word.clone()) }
    for (word in args) {
        if (texts.remove(word) != null) { println("present") }
        println(word)
    }
    println("done")
}
"#;
        run_units(
            source,
            "p.exercise(arrayOf<String>()); p.exercise(arrayOf(\"key\")); p.exercise(arrayOf(\"key\", \"key\"))",
            "done\npresent\nkey\ndone\npresent\nkey\nkey\ndone\n",
        );
    }

    const CALL_OPERANDS: &str = r#"
class Resource(val text: String) { deinit() { println(this.text) } }
fun key(): String { println("key evaluated"); return "key".clone() }
fun make(): Resource? = Resource("plain nullable drop")
fun exercise(): Unit {
    var m = mutableMapOf<String, Resource>()
    m.put("key", Resource("removed drop"))
    if (m.remove(key()) != null) { println("body") }
    val existing = "key".clone()
    if (m.remove(existing) == null) { println(existing) }
    if (make() != null) { println("plain nullable body") }
    var empty = mutableMapOf<String, Resource>()
    if (empty.remove("missing".clone()) == null) { println("empty receiver") }
}
"#;

    #[test]
    fn single_temporary_null_cleanup_call_operands() {
        // Single-file arbitrary nullable factory planning has its own retained boundary.
        let source = CALL_OPERANDS
            .replace(
                "fun make(): Resource? = Resource(\"plain nullable drop\")\n",
                "",
            )
            .replace(
                "    if (make() != null) { println(\"plain nullable body\") }\n",
                "",
            );
        run_single(
            &source,
            "exercise()",
            "key evaluated\nremoved drop\nbody\nkey\nempty receiver\n",
        );
    }

    #[test]
    fn unit_temporary_null_cleanup_call_operands_and_plain_nullable() {
        run_units(
            CALL_OPERANDS,
            "p.exercise()",
            "key evaluated\nremoved drop\nbody\nkey\nplain nullable drop\nplain nullable body\nempty receiver\n",
        );
    }
}

pub(crate) mod container_remove_first_tests {
    //! SPEC-0284: native MutableList.removeFirst head removal, memmove shifting, and empty list abort.

    use super::super::*;
    use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};

    #[test]
    fn unit_container_remove_first_native_primitive_sequential() {
        let analysis = analyze_sources(
            "package p\n",
            r#"package q
        fun entry(): Unit {
            var list = mutableListOf(10, 20, 30)
            if (list.size != 3) { error("initial size mismatch") }

            val first1 = list.removeFirst()
            if (first1 != 10) { error("first removed mismatch") }
            if (list.size != 2) { error("size after removeFirst 1 mismatch") }
            if (list[0] != 20) { error("elem 0 mismatch") }
            if (list[1] != 30) { error("elem 1 mismatch") }

            val first2 = list.removeFirst()
            if (first2 != 20) { error("second removed mismatch") }
            if (list.size != 1) { error("size after removeFirst 2 mismatch") }
            if (list[0] != 30) { error("elem 0 mismatch") }

            val first3 = list.removeFirst()
            if (first3 != 30) { error("third removed mismatch") }
            if (list.size != 0) { error("size after removeFirst 3 mismatch") }

            list.add(99)
            if (list.size != 1) { error("size after add mismatch") }
            if (list[0] != 99) { error("elem 0 mismatch") }

            println("remove-first-primitive-ok")
        }"#,
        );
        run(&analysis, b"remove-first-primitive-ok\n");
    }

    #[test]
    fn unit_container_remove_first_native_move_only_elements_lifecycle() {
        let analysis = analyze_sources(
            r#"package p
        class Resource(val name: String) {
            deinit() {
                println(this.name)
            }
        }"#,
            r#"package q
        import p.Resource
        fun consume(res: Resource): Unit {
            println("consuming")
            println(res.name)
        }
        fun runTest(): Unit {
            var list = mutableListOf<Resource>()
            list.add(Resource("r0"))
            list.add(Resource("r1"))
            list.add(Resource("r2"))
            println("before-remove-first")
            consume(list.removeFirst())
            println("after-consume")
            if (list.size != 2) { error("size mismatch") }
        }
        fun entry(): Unit {
            runTest()
            println("done-all")
        }"#,
        );
        run(
            &analysis,
            b"before-remove-first\nconsuming\nr0\nr0\nafter-consume\nr2\nr1\ndone-all\n",
        );
    }

    #[test]
    fn unit_container_remove_first_native_empty_list_aborts() {
        let analysis = analyze_sources(
            "package p\n",
            r#"package q
        fun entry(): Unit {
            var list = mutableListOf<Int>()
            println("before-empty-remove-first")
            val x = list.removeFirst()
            println("unexpected-reach")
        }"#,
        );
        run_abort(&analysis, b"before-empty-remove-first\n");
    }

    #[test]
    fn unit_container_remove_first_native_pop_and_empty_abort() {
        let analysis = analyze_sources(
            "package p\n",
            r#"package q
        fun entry(): Unit {
            var list = mutableListOf(42)
            val popped = list.removeFirst()
            if (popped != 42) { error("popped mismatch") }
            println("popped-first")
            val x = list.removeFirst()
            println("unexpected-reach")
        }"#,
        );
        run_abort(&analysis, b"popped-first\n");
    }

    fn run(analysis: &UnitAnalysis, expected: &[u8]) {
        let directory = TestDirectory::create();
        let object = directory.join("remove_first.o");
        let executable = directory.join("remove_first");
        emit_native_unit_object(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .expect("source removeFirst emits a verified native object");
        crate::test_support::assert_native_object(&fs::read(&object).unwrap());
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("native link runs");
        assert_diagnostic!(
            linked.status.success(),
            "{}",
            String::from_utf8_lossy(&linked.stderr)
        );
        let output = Command::new(&executable)
            .output()
            .expect("native program runs");
        assert_diagnostic!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, expected);
        assert!(output.stderr.is_empty());
    }

    fn run_abort(analysis: &UnitAnalysis, expected_stdout: &[u8]) {
        let directory = TestDirectory::create();
        let object = directory.join("remove_first_abort.o");
        let executable = directory.join("remove_first_abort");
        emit_native_unit_object(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .expect("source removeFirst emits a verified native object");
        crate::test_support::assert_native_object(&fs::read(&object).unwrap());
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("native link runs");
        assert_diagnostic!(
            linked.status.success(),
            "{}",
            String::from_utf8_lossy(&linked.stderr)
        );
        let output = Command::new(&executable)
            .output()
            .expect("native program runs");
        assert!(
            !output.status.success(),
            "expected abort on empty removeFirst"
        );
        assert_eq!(output.stdout, expected_stdout);
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq_diagnostic!(output.status.signal(), Some(6), "{output:?}");
        }
    }
}

pub(crate) mod container_remove_last_tests {
    //! SPEC-0283: native MutableList.removeLast tail removal and empty list abort.

    use super::super::*;
    use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};

    #[test]
    fn unit_container_remove_last_native_primitive_sequential() {
        let analysis = analyze_sources(
            "package p\n",
            r#"package q
        fun entry(): Unit {
            var list = mutableListOf(10, 20, 30)
            if (list.size != 3) { error("initial size mismatch") }

            val last1 = list.removeLast()
            if (last1 != 30) { error("first removed mismatch") }
            if (list.size != 2) { error("size after removeLast 1 mismatch") }
            if (list[0] != 10) { error("elem 0 mismatch") }
            if (list[1] != 20) { error("elem 1 mismatch") }

            val last2 = list.removeLast()
            if (last2 != 20) { error("second removed mismatch") }
            if (list.size != 1) { error("size after removeLast 2 mismatch") }
            if (list[0] != 10) { error("elem 0 mismatch") }

            val last3 = list.removeLast()
            if (last3 != 10) { error("third removed mismatch") }
            if (list.size != 0) { error("size after removeLast 3 mismatch") }

            list.add(99)
            if (list.size != 1) { error("size after add mismatch") }
            if (list[0] != 99) { error("elem 0 mismatch") }

            println("remove-last-primitive-ok")
        }"#,
        );
        run(&analysis, b"remove-last-primitive-ok\n");
    }

    #[test]
    fn unit_container_remove_last_native_move_only_elements_lifecycle() {
        let analysis = analyze_sources(
            r#"package p
        class Resource(val name: String) {
            deinit() {
                println(this.name)
            }
        }"#,
            r#"package q
        import p.Resource
        fun consume(res: Resource): Unit {
            println("consuming")
            println(res.name)
        }
        fun runTest(): Unit {
            var list = mutableListOf<Resource>()
            list.add(Resource("r0"))
            list.add(Resource("r1"))
            list.add(Resource("r2"))
            println("before-remove-last")
            consume(list.removeLast())
            println("after-consume")
            if (list.size != 2) { error("size mismatch") }
        }
        fun entry(): Unit {
            runTest()
            println("done-all")
        }"#,
        );
        run(
            &analysis,
            b"before-remove-last\nconsuming\nr2\nr2\nafter-consume\nr1\nr0\ndone-all\n",
        );
    }

    #[test]
    fn unit_container_remove_last_native_empty_list_aborts() {
        let analysis = analyze_sources(
            "package p\n",
            r#"package q
        fun entry(): Unit {
            var list = mutableListOf<Int>()
            println("before-empty-remove-last")
            val x = list.removeLast()
            println("unexpected-reach")
        }"#,
        );
        run_abort(&analysis, b"before-empty-remove-last\n");
    }

    #[test]
    fn unit_container_remove_last_native_pop_and_empty_abort() {
        let analysis = analyze_sources(
            "package p\n",
            r#"package q
        fun entry(): Unit {
            var list = mutableListOf(42)
            val popped = list.removeLast()
            if (popped != 42) { error("popped mismatch") }
            println("popped-first")
            val x = list.removeLast()
            println("unexpected-reach")
        }"#,
        );
        run_abort(&analysis, b"popped-first\n");
    }

    fn run(analysis: &UnitAnalysis, expected: &[u8]) {
        let directory = TestDirectory::create();
        let object = directory.join("remove_last.o");
        let executable = directory.join("remove_last");
        emit_native_unit_object(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .expect("source removeLast emits a verified native object");
        crate::test_support::assert_native_object(&fs::read(&object).unwrap());
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("native link runs");
        assert_diagnostic!(
            linked.status.success(),
            "{}",
            String::from_utf8_lossy(&linked.stderr)
        );
        let output = Command::new(&executable)
            .output()
            .expect("native program runs");
        assert_diagnostic!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, expected);
        assert!(output.stderr.is_empty());
    }

    fn run_abort(analysis: &UnitAnalysis, expected_stdout: &[u8]) {
        let directory = TestDirectory::create();
        let object = directory.join("remove_last_abort.o");
        let executable = directory.join("remove_last_abort");
        emit_native_unit_object(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .expect("source removeLast emits a verified native object");
        crate::test_support::assert_native_object(&fs::read(&object).unwrap());
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("native link runs");
        assert_diagnostic!(
            linked.status.success(),
            "{}",
            String::from_utf8_lossy(&linked.stderr)
        );
        let output = Command::new(&executable)
            .output()
            .expect("native program runs");
        assert!(
            !output.status.success(),
            "expected abort on empty removeLast"
        );
        assert_eq!(output.stdout, expected_stdout);
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq_diagnostic!(output.status.signal(), Some(6), "{output:?}");
        }
    }
}

pub(crate) mod container_insert_at_tests {
    //! SPEC-0285: native MutableList.insertAt element insertion and memory shift.

    use super::super::*;
    use crate::test_support::diagnostics::assert_diagnostic;

    #[test]
    fn unit_container_insert_at_native_primitive_sequential() {
        let analysis = analyze_sources(
            "package p\n",
            r#"package q
        fun entry(): Unit {
            var list = mutableListOf<Int>()
            if (list.size != 0) { error("initial size mismatch") }

            list.insertAt(0, 100)
            if (list.size != 1) { error("size after first insert mismatch") }
            if (list[0] != 100) { error("elem 0 mismatch") }

            list.insertAt(0, 50)
            if (list.size != 2) { error("size after prepend mismatch") }
            if (list[0] != 50) { error("elem 0 prepend mismatch") }
            if (list[1] != 100) { error("elem 1 prepend mismatch") }

            list.insertAt(2, 200)
            if (list.size != 3) { error("size after append mismatch") }
            if (list[0] != 50) { error("elem 0 append mismatch") }
            if (list[1] != 100) { error("elem 1 append mismatch") }
            if (list[2] != 200) { error("elem 2 append mismatch") }

            list.insertAt(1, 75)
            if (list.size != 4) { error("size after mid insert mismatch") }
            if (list[0] != 50) { error("elem 0 mid mismatch") }
            if (list[1] != 75) { error("elem 1 mid mismatch") }
            if (list[2] != 100) { error("elem 2 mid mismatch") }
            if (list[3] != 200) { error("elem 3 mid mismatch") }

            list.insertAt(4, 300)
            list.insertAt(0, 25)
            if (list.size != 6) { error("size after growth mismatch") }
            if (list[0] != 25) { error("elem 0 after growth mismatch") }
            if (list[1] != 50) { error("elem 1 after growth mismatch") }
            if (list[2] != 75) { error("elem 2 after growth mismatch") }
            if (list[3] != 100) { error("elem 3 after growth mismatch") }
            if (list[4] != 200) { error("elem 4 after growth mismatch") }
            if (list[5] != 300) { error("elem 5 after growth mismatch") }

            println("insert-at-primitive-ok")
        }"#,
        );
        run(&analysis, b"insert-at-primitive-ok\n");
    }

    #[test]
    fn unit_container_insert_at_native_move_only_elements_lifecycle() {
        let analysis = analyze_sources(
            r#"package p
        class Resource(val name: String) {
            deinit() {
                println(this.name)
            }
        }"#,
            r#"package q
        import p.Resource
        fun runTest(): Unit {
            var list = mutableListOf<Resource>()
            list.add(Resource("r0"))
            list.add(Resource("r2"))
            println("before-insert")
            list.insertAt(1, Resource("r1"))
            println("after-insert")
            if (list.size != 3) { error("size mismatch") }
        }
        fun entry(): Unit {
            runTest()
            println("done-all")
        }"#,
        );
        run(
            &analysis,
            b"before-insert\nafter-insert\nr2\nr1\nr0\ndone-all\n",
        );
    }

    #[test]
    fn unit_container_insert_at_native_out_of_bounds_negative_aborts() {
        let analysis = analyze_sources(
            "package p\n",
            r#"package q
        fun entry(): Unit {
            var list = mutableListOf(1, 2)
            println("before-negative-insert")
            list.insertAt(-1, 99)
            println("unexpected-reach")
        }"#,
        );
        run_abort(&analysis, b"before-negative-insert\n");
    }

    #[test]
    fn unit_container_insert_at_native_out_of_bounds_beyond_size_aborts() {
        let analysis = analyze_sources(
            "package p\n",
            r#"package q
        fun entry(): Unit {
            var list = mutableListOf(1, 2)
            println("before-beyond-insert")
            list.insertAt(3, 99)
            println("unexpected-reach")
        }"#,
        );
        run_abort(&analysis, b"before-beyond-insert\n");
    }

    fn run(analysis: &UnitAnalysis, expected: &[u8]) {
        let directory = TestDirectory::create();
        let object = directory.join("insert_at.o");
        let executable = directory.join("insert_at");
        emit_native_unit_object(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .expect("source insertAt emits a verified native object");
        crate::test_support::assert_native_object(&fs::read(&object).unwrap());
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("native link runs");
        assert_diagnostic!(
            linked.status.success(),
            "{}",
            String::from_utf8_lossy(&linked.stderr)
        );
        let output = Command::new(&executable)
            .output()
            .expect("native program runs");
        assert_diagnostic!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, expected);
    }

    fn run_abort(analysis: &UnitAnalysis, expected_prefix: &[u8]) {
        let directory = TestDirectory::create();
        let object = directory.join("insert_at_abort.o");
        let executable = directory.join("insert_at_abort");
        emit_native_unit_object(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .expect("source insertAt abort emits a verified native object");
        crate::test_support::assert_native_object(&fs::read(&object).unwrap());
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("native link runs");
        assert_diagnostic!(
            linked.status.success(),
            "{}",
            String::from_utf8_lossy(&linked.stderr)
        );
        let output = Command::new(&executable)
            .output()
            .expect("native program runs");
        assert!(!output.status.success());
        assert_diagnostic!(
            output.stdout.starts_with(expected_prefix),
            "stdout was: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
