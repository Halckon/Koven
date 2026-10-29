//! Pending nullable text receivers keep normal and early-return cleanup distinct.
use super::*;

#[test]
fn public_text_nullable_receiver_argument_return_releases_the_pending_view() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
fun observe(stop: Boolean): Unit {
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        val copy = joined.checkedPlus(if (stop) { return } else { "!" })
        if (copy == null) { error("unexpected overflow") }
    }
    println("completed")
}
fun main(): Unit { observe(true); observe(false) }
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    // Both calls allocate joined, but only the completed call allocates its result.
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 3);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"completed\n");
}

#[test]
fn public_text_nullable_receiver_both_argument_branches_cancel_the_call() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
fun observe(stop: Boolean): Unit {
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        val copy = joined.checkedPlus(if (stop) { return } else { return })
    }
}
fun main(): Unit { observe(true); observe(false); println("cancelled") }
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 2);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"cancelled\n");
}

#[test]
fn public_text_nullable_receiver_return_preserves_prior_argument_and_copyable_result() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
import koven.text.sliceBytes
fun observe(input: String, stop: Boolean): Int {
    val joined = input.checkedPlus("b")
    var seen = 0
    if (joined != null) {
        val copy = joined.sliceBytes(
            if (stop) { seen = seen + 1; 0 } else { seen = seen + 2; 0 },
            if (stop) { return seen } else { 1 })
    }
    return seen
}
fun main(): Unit {
    val input = "a".clone()
    if (observe(input, true) != 1 || observe(input, false) != 2) { error("argument order") }
    println(input)
    println("ok")
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 4);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"a\nok\n");
}

#[test]
fn public_text_nullable_receiver_return_keeps_other_if_transfers_rejected() {
    for (source, rejected) in [
        (
            r#"package app
import koven.text.checkedPlus
fun observe(stop: Boolean): String {
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        val copy = joined.checkedPlus(if (stop) { return "early" } else { "!" })
    }
    return "late"
}
fun main(): Unit { println(observe(true)) }
"#,
            "return \"early\"",
        ),
        (
            r#"package app
import koven.text.checkedPlus
fun observe(stop: Boolean): String {
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        if (stop) { return "early" }
        println(joined)
    }
    return "late"
}
fun main(): Unit { println(observe(true)) }
"#,
            "return \"early\"",
        ),
    ] {
        let Err(error) = try_lower_public_text_program(source) else {
            panic!("unqualified If transfer unexpectedly lowered");
        };
        assert_eq!(error.kind, super::super::LoweringErrorKind::UnsupportedNode);
        let span = error.span.unwrap();
        assert_eq!(&source[span.start()..span.end()], rejected);
    }
}

#[test]
fn public_text_nullable_if_both_returns_preserve_the_null_edge() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
import koven.text.byteLength
fun choose(present: Boolean, stop: Boolean): Int {
    val joined = if (present) { "a".checkedPlus("b") } else { null }
    if (joined != null) {
        if (stop) { return joined.byteLength() + 15 } else { return joined.byteLength() + 27 }
    }
    return 0
}
fun main(): Unit {
    if (choose(true, true) != 17 || choose(true, false) != 29 ||
        choose(false, true) != 0 || choose(false, false) != 0) { error("return edge") }
    println("ok")
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 2);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"ok\n");
}

#[test]
fn public_text_nullable_if_returns_retire_only_the_taken_path() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
import koven.text.byteLength
fun unitExit(input: String, stop: Boolean): Unit {
    val joined = input.checkedPlus("b")
    if (joined != null) {
        if (stop) { return }
        println(joined)
    }
    println("completed")
}
fun intExit(input: String, stop: Boolean): Int {
    val joined = input.checkedPlus("b")
    if (joined != null) {
        if (stop) { return joined.byteLength() }
        println(joined)
        return 29
    }
    return 0
}
fun main(): Unit {
    val input = "a".clone()
    unitExit(input, true)
    unitExit(input, false)
    if (intExit(input, true) != 2 || intExit(input, false) != 29) { error("return value") }
    println(input)
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 5);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"ab\ncompleted\nab\na\n");
}
