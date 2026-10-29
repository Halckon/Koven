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

#[test]
fn public_text_nullable_if_returns_independent_string_owners() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
fun direct(stop: Boolean): String {
    val result = "direct".clone()
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        if (stop) { return result }
        println(joined)
    }
    return result
}
fun argument(stop: Boolean): String {
    val result = "argument".clone()
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        val copy = joined.checkedPlus(if (stop) { return result } else { "!" })
        if (copy == null) { error("overflow") }
    }
    return result
}
fun main(): Unit {
    println(direct(true))
    println(direct(false))
    println(argument(true))
    println(argument(false))
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 9);
    crate::native_tests::boxed_enum_tests::assert_success(
        &run,
        b"direct\nab\ndirect\nargument\nargument\n",
    );
}

#[test]
fn public_text_nullable_if_returns_literals_through_existing_nullable_wrap() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
fun direct(stop: Boolean): String? {
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        if (stop) { return "early" }
        println(joined)
    }
    return "late"
}
fun argument(stop: Boolean): String? {
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        val copy = joined.checkedPlus(if (stop) { return "early" } else { "!" })
        if (copy == null) { error("overflow") }
    }
    return "late"
}
fun main(): Unit {
    val first = direct(true)
    if (first != null) { println(first) }
    val second = direct(false)
    if (second != null) { println(second) }
    val third = argument(true)
    if (third != null) { println(third) }
    val fourth = argument(false)
    if (fourth != null) { println(fourth) }
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 5);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"early\nab\nlate\nearly\nlate\n");
}

#[test]
fn public_text_nullable_if_returns_independent_array_and_result_owners() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
import koven.text.toBytes
import koven.text.sliceBytes
import koven.text.TextError
fun bytes(stop: Boolean): Array<UByte> {
    val result = "é".toBytes()
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        if (stop) { return result }
        println(joined)
    }
    return result
}
fun sliced(stop: Boolean): Result<String, TextError> {
    val result = "slice".sliceBytes(0, 5)
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        val copy = joined.checkedPlus(if (stop) { return result } else { "!" })
        if (copy == null) { error("overflow") }
    }
    return result
}
fun main(): Unit {
    val first = bytes(true)
    val second = bytes(false)
    val lead: UByte = 195u
    val continuation: UByte = 169u
    if (first.size != 2 || first[0] != lead || first[1] != continuation ||
        second.size != 2 || second[0] != lead || second[1] != continuation) { error("bytes") }
    val third = sliced(true)
    when (third) {
        is Ok -> println(third.success)
        is Err -> error("slice")
    }
    val fourth = sliced(false)
    when (fourth) {
        is Ok -> println(fourth.success)
        is Err -> error("slice")
    }
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 9);
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"ab\nslice\nslice\n");
}

#[test]
fn public_text_nullable_if_wraps_allocated_returned_strings() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
fun direct(stop: Boolean): String? {
    val result = "direct".clone()
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        if (stop) { return result }
        println(joined)
    }
    return result
}
fun argument(stop: Boolean): String? {
    val result = "argument".clone()
    val joined = "a".checkedPlus("b")
    if (joined != null) {
        val copy = joined.checkedPlus(if (stop) { return result } else { "!" })
        if (copy == null) { error("overflow") }
    }
    return result
}
fun main(): Unit {
    val first = direct(true)
    if (first != null) { println(first) }
    val second = direct(false)
    if (second != null) { println(second) }
    val third = argument(true)
    if (third != null) { println(third) }
    val fourth = argument(false)
    if (fourth != null) { println(fourth) }
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 9);
    crate::native_tests::boxed_enum_tests::assert_success(
        &run,
        b"direct\nab\ndirect\nargument\nargument\n",
    );
}

#[test]
fn public_text_nullable_if_delivers_its_narrowed_owner_on_each_return_edge() {
    let (program, entry) = lower_public_text_program(
        r#"package app
import koven.text.checkedPlus
fun direct(present: Boolean, stop: Boolean): String {
    val joined = if (present) { "a".checkedPlus("b") } else { null }
    if (joined != null) {
        if (stop) { return ((joined)) }
        println(joined)
        return joined
    }
    return "absent"
}
fun wrapped(present: Boolean, stop: Boolean): String? {
    val joined = if (present) { "c".checkedPlus("d") } else { null }
    if (joined != null) {
        if (stop) { return ((joined)) }
        println(joined)
        return joined
    }
    return joined
}
fun main(): Unit {
    println(direct(true, true))
    println(direct(true, false))
    println(direct(false, true))
    println(direct(false, false))
    val first = wrapped(true, true)
    if (first != null) { println(first) } else { error("missing first") }
    val second = wrapped(true, false)
    if (second != null) { println(second) } else { error("missing second") }
    val third = wrapped(false, true)
    if (third != null) { error("unexpected third") }
    val fourth = wrapped(false, false)
    if (fourth != null) { error("unexpected fourth") }
    println("null")
}
"#,
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 4);
    crate::native_tests::boxed_enum_tests::assert_success(
        &run,
        b"ab\nab\nab\nabsent\nabsent\ncd\ncd\ncd\nnull\n",
    );
}
