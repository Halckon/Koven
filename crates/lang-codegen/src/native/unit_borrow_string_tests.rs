//! SPEC-0274: cross-file Borrow String reads and independent concat ownership.

use super::*;
use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};

#[test]
fn unit_borrow_string_binary_native_preserves_complete_bytes_and_source_reuse() {
    let analysis = analyze_sources(
        r#"package p
        fun same(a: String, b: String): Boolean = ((a)) == (b)
        fun different(a: String, b: String): Boolean = (a) != ((b))
        fun decorated(text: String): String = (text) + "!"
        fun prefix(text: String): String = ">" + (text)
        fun mixed(text: String, own suffix: String): String = text + suffix
        fun choose(text: String, own flag: Boolean): String {
            val joined = (text) + (if (flag) "右\0" else "")
            println(text)
            return joined
        }"#,
        r#"package q
        fun entry(): Unit {
            val source = "界\0" + "é"
            val empty = "" + ""
            if (!p.same(source, "界\0é")) { error("dynamic/static equality") }
            if (!p.same(empty, "")) { error("empty equality") }
            if (!p.different(source, "界\0ê")) { error("bytes after NUL matter") }
            if (p.different(source, source)) { error("repeated loan must preserve source") }
            println(p.decorated(source))
            println(p.prefix(source))
            println(p.mixed(source, "?" + "尾"))
            println(p.decorated(empty))
            println(p.choose(source, true))
            println(p.choose(source, false))
            println(source)
            println(empty)
        }"#,
    );
    let directory = TestDirectory::create();
    let object = directory.join("borrow-string.o");
    let executable = directory.join("borrow-string");
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
    .expect("Borrow String operations emit a native object");
    crate::test_support::assert_native_object(&fs::read(&object).unwrap());
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().unwrap();
    assert_success(
        &run,
        "界\0é!\n>界\0é\n界\0é?尾\n!\n界\0é\n界\0é右\0\n界\0é\n界\0é\n界\0é\n\n".as_bytes(),
    );
}

#[test]
fn unit_borrow_string_comparisons_native_preserve_loans_after_both_rhs_branches() {
    let analysis = analyze_sources(
        r#"package p
        fun sameAfter(text: String, own flag: Boolean): Boolean {
            val result = (text) == (if (flag) "界\0é" else "界\0ê")
            println(text)
            return result
        }
        fun differentAfter(text: String, own flag: Boolean): Boolean {
            val result = (text) != (if (flag) "界\0é" else "界\0ê")
            println(text)
            return result
        }"#,
        r#"package q
        fun entry(): Unit {
            val source = "界\0" + "é"
            if (!p.sameAfter(source, true)) { error("equal true branch") }
            if (p.sameAfter(source, false)) { error("equal false branch") }
            if (p.differentAfter(source, true)) { error("not-equal true branch") }
            if (!p.differentAfter(source, false)) { error("not-equal false branch") }
            println(source)
        }"#,
    );
    let directory = TestDirectory::create();
    let object = directory.join("borrow-string-comparisons.o");
    let executable = directory.join("borrow-string-comparisons");
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
    .expect("RHS CFG String comparisons emit a native object");
    crate::test_support::assert_native_object(&fs::read(&object).unwrap());
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().unwrap();
    assert_success(&run, "界\0é\n界\0é\n界\0é\n界\0é\n界\0é\n".as_bytes());
}

#[test]
fn unit_borrow_string_concat_result_and_source_drop_independently() {
    for source_first in [false, true] {
        let tail = if source_first {
            "p.consume(source)\nprintln(joined)\np.consume(joined)"
        } else {
            "p.consume(joined)\nprintln(source)\np.consume(source)"
        };
        let analysis = analyze_sources(
            r#"package p
            fun same(a: String, b: String): Boolean = a == b
            fun decorated(text: String): String = text + "!"
            fun consume(own text: String): Unit { println(text) }"#,
            &format!(
                r#"package q
            fun entry(): Unit {{
                val source = "界\0" + "é"
                if (!p.same(source, "界\0é")) {{ error("first read") }}
                val joined = p.decorated(source)
                if (!p.same(source, "界\0é")) {{ error("source after concat") }}
                {tail}
            }}"#
            ),
        );
        let (program, entry) = lower_scalar_unit_with_entry(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
        )
        .expect("Borrow String concat uses source without transferring its owner");
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        // Allocation 0 is the dynamic source; allocation 1 is the concat result.
        let order = if source_first { [0, 1] } else { [1, 0] };
        let run = run_counted_allocations_in_order(&llvm, &order);
        let stdout = if source_first {
            "界\0é\n界\0é!\n界\0é!\n"
        } else {
            "界\0é!\n界\0é\n界\0é\n"
        };
        assert_success(&run, stdout.as_bytes());
    }
}
