//! Borrowed String proofs after guard control flow.
use super::emit_link_and_run;
use crate::native_tests::boxed_enum_tests::assert_success;

#[test]
fn native_borrowed_nullable_string_survives_a_null_guard_return() {
    let source = r#"
fun inspect(value: String?): Unit {
    if (value == null) { println("absent"); return }
    println(value)
    println(value)
}
fun main(): Unit {
    inspect(null)
    inspect("")
    val value: String? = "kept"
    inspect(value)
    inspect(value)
}
"#;
    for condition in ["value == null", "!(value != null)"] {
        let source = source.replace("value == null", condition);
        let output = emit_link_and_run("borrowed-nullable-string-guard.ko", &source, "main");
        assert_success(&output, b"absent\n\n\nkept\nkept\nkept\nkept\n");
    }
}

#[test]
fn native_borrowed_nullable_string_joins_two_guard_return_paths() {
    let source = r#"
fun inspect(value: String?, gate: Boolean): Unit {
    if (gate) {
        if (value == null) { return }
        println("left")
    } else {
        if (!(value != null)) { return }
        println("right")
    }
    println(value)
}
fun main(): Unit {
    inspect(null, true)
    inspect(null, false)
    inspect("", true)
    val value: String? = "owned"
    inspect(value, false)
    inspect(value, true)
}
"#;
    let output = emit_link_and_run("borrowed-nullable-string-two-guards.ko", source, "main");
    assert_success(&output, b"left\n\nright\nowned\nleft\nowned\n");
}

#[test]
fn native_borrowed_nullable_string_keeps_the_inherited_proof_across_a_loop() {
    let source = r#"
fun inspect(value: String?, gate: Boolean, limit: Int): Unit {
    if (value == null) { return }
    var i = 0
    while (i < limit) {
        i = i + 1
        if (gate) {
            if (value == null) { return }
            println("left")
        } else {
            println("right")
        }
        println(value)
    }
    println(value)
}
fun main(): Unit {
    inspect(null, true, 2)
    inspect("", true, 0)
    val value: String? = "owned"
    inspect(value, true, 2)
    inspect(value, false, 1)
}
"#;
    let output = emit_link_and_run("borrowed-nullable-string-loop-guard.ko", source, "main");
    assert_success(
        &output,
        b"\nleft\nowned\nleft\nowned\nowned\nright\nowned\nowned\n",
    );
}

#[test]
fn native_borrowed_nullable_string_survives_a_subjectless_when() {
    let source = r#"
fun inspect(value: String?, gate: Boolean): Unit {
    when {
        gate -> { if (value == null) { return }; println("left") }
        else -> { if (!(value != null)) { return }; println("right") }
    }
    println(value)
}
fun main(): Unit {
    inspect(null, true)
    inspect(null, false)
    inspect("", true)
    val value: String? = "owned"
    inspect(value, false)
    inspect(value, true)
}
"#;
    let output = emit_link_and_run("borrowed-nullable-string-when-guard.ko", source, "main");
    assert_success(&output, b"left\n\nright\nowned\nleft\nowned\n");
}

#[test]
fn native_borrowed_nullable_string_leaves_a_single_continuing_when_arm() {
    let source = r#"
fun inspect(value: String?, gate: Boolean): Unit {
    when {
        gate -> { if (value == null) { return }; println("kept") }
        else -> { return }
    }
    println(value)
}
fun main(): Unit {
    inspect(null, true)
    val value: String? = "twice"
    inspect(value, true)
    inspect(value, false)
}
"#;
    let output = emit_link_and_run(
        "borrowed-nullable-string-when-single-arm.ko",
        source,
        "main",
    );
    assert_success(&output, b"kept\ntwice\n");
}

#[test]
fn native_borrowed_nullable_string_keeps_proof_after_evaluating_a_when_selector() {
    let source = r#"
fun inspect(value: String?, gate: Boolean): Unit {
    when {
        (if (value == null) { return } else { gate }) -> { println("hit") }
    }
    println(value)
}
fun main(): Unit {
    inspect(null, true)
    inspect("", false)
    val value: String? = "owned"
    inspect(value, false)
    inspect(value, true)
}
"#;
    let output = emit_link_and_run(
        "borrowed-nullable-string-when-selector-guard.ko",
        source,
        "main",
    );
    assert_success(&output, b"\nowned\nhit\nowned\n");
}

#[test]
fn native_borrowed_nullable_string_accepts_a_nonempty_literal_argument() {
    let source = r#"
fun inspect(value: String?): Unit {
    if (value == null) { println("absent"); return }
    println(value)
}
fun direct(): Unit { inspect("payload") }
fun grouped(): Unit { inspect(value = ("grouped")) }
fun main(): Unit {
    inspect(null)
    inspect("")
    direct()
    grouped()
    println("after")
}
"#;
    let output = emit_link_and_run("borrowed-nullable-string-literal.ko", source, "main");
    assert_success(&output, b"absent\n\npayload\ngrouped\nafter\n");
}

#[test]
fn native_borrowed_nullable_string_temporary_arguments_drop_in_reverse_order() {
    use crate::native_tests::boxed_enum_tests::{lower_to_llvm, run_counted_allocations_in_order};

    let source = r#"
fun inspect(first: String?, second: String?): Unit {
    if (first != null && second != null) {
        println(first)
        println(second)
    }
}
fun entry(): Unit { inspect(second = "second".clone(), first = "first".clone()) }
"#;
    let llvm = lower_to_llvm("borrowed-nullable-string-temporary-order.ko", source);
    assert_success(
        &run_counted_allocations_in_order(&llvm, &[1, 0]),
        b"first\nsecond\n",
    );
}

#[test]
fn native_borrowed_nullable_string_temporary_is_released_if_later_argument_returns() {
    use crate::native_tests::boxed_enum_tests::{lower_to_llvm, run_counted_allocations_in_order};

    let source = r#"
fun inspect(value: String?, flag: Boolean): Unit {
    if (value != null) { println(value) }
    if (flag) { println("called") }
}
fun attempt(stop: Boolean): Unit {
    inspect("payload".clone(), if (stop) { return } else { true })
}
fun entry(): Unit {
    attempt(true)
    attempt(false)
}
"#;
    let llvm = lower_to_llvm("borrowed-nullable-string-temporary-early-return.ko", source);
    assert_success(
        &run_counted_allocations_in_order(&llvm, &[0, 1]),
        b"payload\ncalled\n",
    );
}

#[test]
fn native_nullable_string_borrows_a_named_non_null_source_without_moving_it() {
    use crate::native_tests::boxed_enum_tests::{lower_to_llvm, run_counted_allocations_in_order};

    let source = r#"
fun inspect(value: String?): Unit {
    if (value == null) { println("absent"); return }
    println(value)
}
fun forward(value: String): Unit { inspect(value = (value)) }
fun view(source: String): borrow String from source = source
fun take(own value: String): Unit { println(value) }
fun entry(): Unit {
    val value = "owned".clone()
    inspect(value)
    forward(value)
    { borrow val item = view(value); inspect(item) }
    take(value)
}
"#;
    let llvm = lower_to_llvm("borrowed-nullable-named-string.ko", source);
    assert_success(
        &run_counted_allocations_in_order(&llvm, &[0]),
        b"owned\nowned\nowned\nowned\n",
    );
}

#[test]
fn native_nullable_string_view_ends_before_returning_another_borrow() {
    use crate::native_tests::boxed_enum_tests::{lower_to_llvm, run_counted_allocations_in_order};

    let source = r#"
fun inspect(value: String?, source: String): borrow String from source {
    if (!(value == null)) { println(value); return source }
    return source
}
fun take(own value: String): Unit { println(value) }
fun entry(): Unit {
    val source = "source".clone()
    val value: String? = "owned".clone()
    { borrow val result = inspect(null, source); println(result) }
    { borrow val result = inspect(value, source); println(result) }
    take(source)
}
"#;
    for (path, read) in [
        ("direct", "println(value)"),
        ("alias", "borrow val item = value; println(item)"),
        ("result", "borrow val item = view((value)); println(item)"),
    ] {
        let source = format!(
            "fun view(source: String): borrow String from source = source\n{}",
            source.replace("println(value)", read)
        );
        let llvm = lower_to_llvm(&format!("guarded-string-{path}.ko"), &source);
        // The nullable argument reaches its last use at inspect; source survives take.
        assert_success(
            &run_counted_allocations_in_order(&llvm, &[1, 0]),
            b"source\nowned\nsource\nsource\n",
        );
    }
}

#[test]
fn native_nullable_string_views_end_before_forwarding_another_borrow() {
    use crate::native_tests::boxed_enum_tests::{lower_to_llvm, run_counted_allocations_in_order};

    let source = r#"
fun view(source: String): borrow String from source = source
fun inspect(value: String?, source: String): borrow String from source {
    if (value != null) {
        borrow val item = view(value)
        println(item)
        return EARLY
    }
    return TAIL
}
fun take(own value: String): Unit { println(value) }
fun entry(): Unit {
    val source = "source".clone()
    val value: String? = "owned".clone()
    { borrow val result = inspect(null, source); println(result) }
    { borrow val result = inspect(value, source); println(result) }
    take(source)
}
"#;
    for (name, early, tail) in [
        ("while-active", "(view((source)))", "source"),
        ("after-scope", "source", "view(source)"),
        ("both", "view(source)", "view(source)"),
    ] {
        let source = source.replace("EARLY", early).replace("TAIL", tail);
        let llvm = lower_to_llvm(&format!("forwarded-string-{name}.ko"), &source);
        assert_success(
            &run_counted_allocations_in_order(&llvm, &[1, 0]),
            b"source\nowned\nsource\nsource\n",
        );
    }
}

#[test]
fn native_nullable_string_bindings_retire_before_the_payload_view() {
    use crate::native_tests::boxed_enum_tests::{lower_to_llvm, run_counted_allocations_in_order};

    let source = r#"
fun view(source: String): borrow String from source = source
fun inspect(value: String?): Unit {
    if (value != null) { borrow val item = PAYLOAD; println(item); EXIT }
    println("done")
}
fun entry(): Unit {
    val value: String? = "owned".clone()
    inspect(null)
    inspect(value)
}
"#;
    for (name, payload) in [("alias", "value"), ("result", "view(value)")] {
        for (mode, exit, stdout) in [
            ("return", "return", b"done\nowned\n".as_slice()),
            ("scope", "", b"done\nowned\ndone\n".as_slice()),
        ] {
            let source = source.replace("PAYLOAD", payload).replace("EXIT", exit);
            let llvm = lower_to_llvm(&format!("nullable-string-{name}-{mode}.ko"), &source);
            assert_success(&run_counted_allocations_in_order(&llvm, &[0]), stdout);
        }
    }
}

#[test]
fn native_nullable_string_arguments_unwind_before_returning_an_entry_loan() {
    use crate::native_tests::boxed_enum_tests::{lower_to_llvm, run_counted_allocations_in_order};

    let source = r#"
fun view(source: String): borrow String from source = source
fun inspect(value: String?, flag: Boolean): Unit {
    if (value != null) { println(value) }
    if (flag) { println("called") }
}
fun choose(source: String, stop: Boolean): borrow String from source {
    inspect("payload".clone(), if (stop) { return RESULT } else { true })
    return source
}
fun take(own value: String): Unit { println(value) }
fun entry(): Unit {
    val source = "owner".clone()
    { borrow val result = choose(source, true); println(result) }
    { borrow val result = choose(source, false); println(result) }
    take(source)
}
"#;
    for (name, result) in [("direct", "(source)"), ("forwarded", "view(source)")] {
        let source = source.replace("RESULT", result);
        let llvm = lower_to_llvm(
            &format!("nullable-string-pending-argument-{name}.ko"),
            &source,
        );
        assert_success(
            &run_counted_allocations_in_order(&llvm, &[1, 2, 0]),
            b"owner\npayload\ncalled\nowner\nowner\n",
        );
    }
}

#[test]
fn native_nullable_string_forwarding_preserves_the_entry_loan_across_arguments() {
    use crate::native_tests::boxed_enum_tests::{lower_to_llvm, run_counted_allocations_in_order};

    let source = r#"
fun view(source: String, flag: Boolean): borrow String from source {
    if (flag) { println("true") } else { println("false") }
    return source
}
fun choose(source: String, stop: Boolean): borrow String from source {
    return view(source, ARGUMENT)
}
fun take(own value: String): Unit { println(value) }
fun entry(): Unit {
    val source = "owner".clone()
    { borrow val result = choose(source, true); println(result) }
    { borrow val result = choose(source, false); println(result) }
    take(source)
}
"#;
    for (kind, argument) in [
        ("join", "if (stop) { true } else { false }"),
        (
            "early-exit",
            "if (stop) { return view(source, true) } else { false }",
        ),
    ] {
        let source = source.replace("ARGUMENT", argument);
        let llvm = lower_to_llvm(&format!("forward-entry-argument-{kind}.ko"), &source);
        assert_success(
            &run_counted_allocations_in_order(&llvm, &[0]),
            b"true\nowner\nfalse\nowner\nowner\n",
        );
    }
}

#[test]
fn native_nullable_string_forwarding_unwinds_pending_auxiliary_owners() {
    use crate::native_tests::boxed_enum_tests::{lower_to_llvm, run_counted_allocations_in_order};

    let source = r#"
fun view(source: String): borrow String from source = source
fun extra(MODE aux: String, source: String, flag: Boolean): borrow String from source {
    println(aux)
    if (flag) { println("called") }
    return source
}
fun choose(source: String, stop: Boolean): borrow String from source {
    return extra("payload".clone(), source, if (stop) { return view(source) } else { true })
}
fun take(own value: String): Unit { println(value) }
fun entry(): Unit {
    val source = "owner".clone()
    { borrow val result = choose(source, true); println(result) }
    { borrow val result = choose(source, false); println(result) }
    take(source)
}
"#;
    for kind in ["borrow", "own", "nullable-temp", "nullable-place"] {
        let text = source.replace("MODE ", if kind == "own" { "own " } else { "" });
        let text = if kind.starts_with("nullable-") {
            text.replace("aux: String,", "aux: String?,")
                .replace("println(aux)", "if (aux != null) { println(aux) }")
        } else {
            text
        };
        let source = if kind == "nullable-place" {
            text.replace(
                "return extra(\"payload\".clone(),",
                "val payload = \"payload\".clone()\n    return extra(payload,",
            )
        } else {
            text
        };
        let llvm = lower_to_llvm(
            &format!("forward-entry-pending-argument-{kind}.ko"),
            &source,
        );
        assert_success(
            &run_counted_allocations_in_order(&llvm, &[1, 2, 0]),
            b"owner\npayload\ncalled\nowner\nowner\n",
        );
    }
}

#[test]
fn native_nullable_string_borrow_return_runs_only_the_abandoned_call_prefix() {
    use crate::native_tests::boxed_enum_tests::{lower_to_llvm, run_counted_allocations_in_order};

    let source = r#"
fun tick(): String { println("tick"); return "payload".clone() }
fun mark(): Boolean { println("mark"); return true }
fun late(): Boolean { println("late"); return true }
fun view(source: String): borrow String from source = source
fun never_called(source: String, last: Boolean, marker: Boolean, MODE prefix: TYPE, branch: Boolean): borrow String from source {
    println("unexpected")
    return source
}
fun choose(source: String, stop: Boolean): borrow String from source {
    return (never_called(prefix = tick(), marker = mark(), source = source,
        branch = if (stop) { return source } else { return view(source) }, last = late()))
}
fun take(own value: String): Unit { println(value) }
fun entry(): Unit {
    val source = "owner".clone()
    { borrow val result = choose(source, true); println(result) }
    { borrow val result = choose(source, false); println(result) }
    take(source)
}
"#;
    for (name, mode, ty) in [
        ("borrow", "", "String"),
        ("own", "own ", "String"),
        ("nullable", "", "String?"),
    ] {
        let source = source.replace("MODE ", mode).replace("TYPE", ty);
        let llvm = lower_to_llvm(&format!("abandoned-borrow-result-{name}.ko"), &source);
        assert!(!llvm.contains(".never_called."));
        assert!(!llvm.contains(".late."));
        assert_success(
            &run_counted_allocations_in_order(&llvm, &[1, 2, 0]),
            b"tick\nmark\nowner\ntick\nmark\nowner\nowner\n",
        );
    }
}

#[test]
fn native_nullable_string_result_retires_on_each_boolean_branch_exit() {
    use crate::native_tests::boxed_enum_tests::{lower_to_llvm, run_counted_allocations_in_order};

    let source = r#"
fun view(source: String): borrow String from source = source
fun gate(): Boolean { println("tested"); return true }
fun ordinary(stop: Boolean): Unit {
    val source = "ordinary".clone()
    borrow val item = view(source)
    if (stop && gate()) {
        println(item)
        return
    }
    println(item)
    println("joined")
}
fun borrowed(source: String, outer: Boolean, stop: Boolean): borrow String from source {
    borrow val item = view(source)
    if (outer) {
        if ((stop) == true) { println(item); return view(source) }
    }
    println(item)
    return source
}
fun inside(outer: Boolean, stop: Boolean): Unit {
    val source = "inside".clone()
    if (outer) {
        borrow val item = view(source)
        if (stop) { println(item); return }
        println(item)
    } else {
        if (stop == true) { if (!outer) { return } }
    }
    println("after")
    return
}
fun terminating_arm(outer: Boolean): Unit {
    val source = "exit".clone()
    if (outer) {
        borrow val item = view(source)
        println(item)
        return
    }
    println("skip")
    return
}
fun take(own value: String): Unit { println(value) }
fun entry(): Unit {
    ordinary(true)
    ordinary(false)
    inside(false, false)
    inside(false, true)
    inside(true, true)
    inside(true, false)
    terminating_arm(true)
    terminating_arm(false)
    val source = "borrowed".clone()
    { borrow val result = borrowed(source, true, true); println(result) }
    { borrow val result = borrowed(source, true, false); println(result) }
    take(source)
}
"#;
    let llvm = lower_to_llvm("lexical-string-branch-exits.ko", source);
    assert_success(
        &run_counted_allocations_in_order(&llvm, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
        b"tested\nordinary\nordinary\njoined\nafter\ninside\ninside\nafter\nexit\nskip\nborrowed\nborrowed\nborrowed\nborrowed\nborrowed\n",
    );
}

#[test]
fn native_string_result_loan_follows_boolean_when_arms() {
    use crate::native_tests::boxed_enum_tests::{lower_to_llvm, run_counted_allocations_in_order};

    let source = r#"
fun view(source: String): borrow String from source = source
fun subject(flag: Boolean): Boolean { println("subject"); return flag }
fun route(source: String, flag: Boolean): borrow String from source {
    borrow val item = view(source)
    when (subject(flag)) {
        true -> { println(item); return view(source) };
        false -> { println("continue") }
    }
    println(item)
    return source
}
fun inside(flag: Boolean, stop: Boolean): Unit {
    val source = "inside".clone()
    when ((flag)) {
        true -> {
            borrow val item = view(source)
            if (stop == true) { println(item); return }
            println(item)
        };
        false -> { if (stop) { return } }
    }
    println("joined")
    return
}
fun selector(flag: Boolean): Boolean {
    if (flag) { println("test true") } else { println("test false") }
    return flag
}
fun ordered(flag: Boolean): Unit {
    val source = "ordered".clone()
    borrow val item = view(source)
    when (flag) {
        selector(false) -> { println(item); return };
        selector(true) -> { println("matched") };
        else -> { println("unexpected") }
    }
    println(item)
}
fun subjectless(flag: Boolean): Unit {
    val source = "subjectless".clone()
    borrow val item = view(source)
    when { flag -> { println(item); return }; else -> { println("otherwise") } }
    println(item)
}
fun skipped(): Boolean? { println("unexpected selector"); return null }
fun closed(flag: Boolean): Unit {
    val source = "closed".clone()
    borrow val item = view(source)
    when (flag) {
        true, false, skipped() -> { println(item) };
        else -> { return }
    }
    println(item)
}
fun take(own value: String): Unit { println(value) }
fun entry(): Unit {
    inside(false, true)
    inside(true, true)
    inside(true, false)
    ordered(false)
    ordered(true)
    subjectless(true)
    subjectless(false)
    closed(true)
    closed(false)
    val source = "owner".clone()
    { borrow val result = route(source, true); println(result) }
    { borrow val result = route(source, false); println(result) }
    take(source)
}
"#;
    let llvm = lower_to_llvm("lexical-string-boolean-when.ko", source);
    assert_success(
        &run_counted_allocations_in_order(&llvm, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]),
        b"inside\ninside\njoined\ntest false\nordered\ntest false\ntest true\nmatched\nordered\nsubjectless\notherwise\nsubjectless\nclosed\nclosed\nclosed\nclosed\nsubject\nowner\nowner\nsubject\ncontinue\nowner\nowner\nowner\n",
    );
}

#[test]
fn native_string_result_loan_follows_integer_when_arms() {
    use crate::native_tests::boxed_enum_tests::{lower_to_llvm, run_counted_allocations_in_order};

    let source = r#"
fun view(source: String): borrow String from source = source
fun subject(n: Int): Int { println("subject"); return n }
fun bound(marker: String, n: Int): Int { println(marker); return n }
fun route(source: String, n: Int): borrow String from source {
    borrow val item = view(source)
    when (subject(n + 0)) {
        bound("exact", 0) -> { println(item); return view(source) };
        in bound("start", 1)..<bound("end", 2 + 1) -> { println("range") };
        !in 0..9 -> { println(item); return source };
        else -> { println("else") }
    }
    println(item)
    return source
}
fun inside(n: UInt): Unit {
    val source = "inside".clone()
    when (n) {
        1u -> {
            borrow val item = view(source)
            println(item)
            return
        };
        else -> { println("otherwise") }
    }
    println("joined")
    return
}
fun take(own value: String): Unit { println(value) }
fun entry(): Unit {
    inside(1u)
    inside(2u)
    val source = "owner".clone()
    { borrow val result = route(source, 0); println(result) }
    { borrow val result = route(source, 2); println(result) }
    { borrow val result = route(source, 3); println(result) }
    { borrow val result = route(source, 10); println(result) }
    take(source)
}
"#;
    let llvm = lower_to_llvm("lexical-string-integer-when.ko", source);
    assert_success(
        &run_counted_allocations_in_order(&llvm, &[0, 1, 2]),
        b"inside\notherwise\njoined\nsubject\nexact\nowner\nowner\nsubject\nexact\nstart\nend\nrange\nowner\nowner\nsubject\nexact\nstart\nend\nelse\nowner\nowner\nsubject\nexact\nstart\nend\nowner\nowner\nowner\n",
    );
}

#[test]
fn native_string_result_loan_crosses_text_and_copy_enum_when() {
    use crate::native_tests::boxed_enum_tests::{lower_to_llvm, run_counted_allocations_in_order};

    let source = r#"
enum class Tag { Mark(n: Int), Other }
fun view(source: String): borrow String from source = source
fun text(source: String, own key: String): borrow String from source {
    borrow val item = view(source)
    when ((key)) {
        ("hit") -> { println(item); return view(source) };
        else -> { println("miss") }
    }
    println(item)
    return source
}
fun tagged(source: String, tag: Tag): Unit {
    borrow val item = view(source)
    when (tag) {
        is Tag.Mark -> { println(item); return };
        (Tag.Other) -> { println("other") }
    }
    println(item)
    return
}
fun take(own value: String): Unit { println(value) }
fun entry(): Unit {
    val source = "owner".clone()
    { borrow val result = text(source, "hit".clone()); println(result) }
    { borrow val result = text(source, "miss".clone()); println(result) }
    tagged(source, Tag.Mark(7))
    tagged(source, Tag.Other)
    take(source)
}
"#;
    let llvm = lower_to_llvm("lexical-string-text-enum-when.ko", source);
    assert_success(
        &run_counted_allocations_in_order(&llvm, &[1, 2, 0]),
        b"owner\nowner\nmiss\nowner\nowner\nowner\nother\nowner\nowner\n",
    );
}
