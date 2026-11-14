//! SPEC-0229: exact extended integer values survive object emission, linking and execution.
use super::emit_link_and_run;

// Each expected value deliberately uses the pre-existing decimal spelling.
const VALUES: &[(&str, &str, &str)] = &[
    ("Int", "0x2a", "42"),
    ("Int", "0X2A", "42"),
    ("Int", "0b10_1010", "42"),
    ("Int", "0B101010", "42"),
    ("Int", "1_234_567", "1234567"),
    ("Int", "0x1f", "31"),
    ("Byte", "0x7_f", "127"),
    ("Short", "-0X8_000", "-32768"),
    ("UByte", "0b1111_1111u", "255u"),
    ("UShort", "0B1111_1111_1111_1111U", "65535u"),
    ("Long", "0x1_0000_0000L", "4294967296L"),
    (
        "Long",
        "0B1_00000000_00000000_00000000_00000000L",
        "4294967296L",
    ),
    ("UInt", "0xFFFF_FFFFu", "4294967295u"),
    ("UInt", "0XFFFF_FFFFU", "4294967295u"),
    ("UInt", "4_294_967_295U", "4294967295u"),
    ("ULong", "0xFFFF_FFFF_FFFF_FFFFuL", "18446744073709551615uL"),
    ("ULong", "0XFFFF_FFFF_FFFF_FFFFUL", "18446744073709551615uL"),
    (
        "ULong",
        "18_446_744_073_709_551_615uL",
        "18446744073709551615uL",
    ),
    (
        "ULong",
        "0b11111111_11111111_11111111_11111111_11111111_11111111_11111111_11111111UL",
        "18446744073709551615uL",
    ),
    ("Int", "-0x8000_0000", "-2147483648"),
    (
        "Int",
        "-0B10000000_00000000_00000000_00000000",
        "-2147483648",
    ),
    ("Int", "-2_147_483_648", "-2147483648"),
    ("Long", "-0X8000_0000_0000_0000L", "-9223372036854775808L"),
    (
        "Long",
        "-0b10000000_00000000_00000000_00000000_00000000_00000000_00000000_00000000L",
        "-9223372036854775808L",
    ),
    (
        "Long",
        "-9_223_372_036_854_775_808L",
        "-9223372036854775808L",
    ),
];

#[test]
fn numeric_literals_run_with_decimal_value_oracles() {
    let functions = VALUES
        .iter()
        .enumerate()
        .map(|(index, (ty, literal, _))| format!("fun value{index}(): {ty} = {literal}\n"))
        .collect::<String>();
    let checks = VALUES
        .iter()
        .enumerate()
        .map(|(index, (ty, _, expected))| {
            format!(
                "val expected{index}: {ty} = {expected}\nif (value{index}() == expected{index}) {{ println(\"success-{index}\") }}\n"
            )
        })
        .collect::<String>();
    let source = format!("{functions}fun entry(): Unit {{\n{checks}}}\n");
    assert_success(&source, expected_stdout());
}

#[test]
fn numeric_literal_constants_run_with_decimal_value_oracles() {
    let declarations = VALUES
        .iter()
        .enumerate()
        .map(|(index, (ty, literal, _))| format!("const val VALUE{index}: {ty} = {literal}\n"))
        .collect::<String>();
    let checks = VALUES
        .iter()
        .enumerate()
        .map(|(index, (ty, _, expected))| {
            format!(
                "val expected{index}: {ty} = {expected}\nif (VALUE{index} == expected{index}) {{ println(\"success-{index}\") }}\n"
            )
        })
        .collect::<String>();
    let source = format!(
        "{declarations}const val DERIVED: Int = VALUE5 + 1_1\nfun entry(): Unit {{\n{checks}if (DERIVED == 42) {{ println(\"success-derived\") }}\n}}\n"
    );
    assert_success(&source, expected_stdout() + "success-derived\n");
}

fn expected_stdout() -> String {
    (0..VALUES.len())
        .map(|index| format!("success-{index}\n"))
        .collect()
}

fn assert_success(source: &str, expected: String) {
    let run = emit_link_and_run("numeric-literals.ko", source, "entry");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, expected.as_bytes());
    assert!(run.stderr.is_empty(), "{run:?}");
}

pub(crate) mod bitwise_tests {
    use super::super::emit_link_and_run;

    #[test]
    fn bitwise_native_width_boundaries_masked_counts_and_eager_operands() {
        let (functions, checks, expected) = crate::bitwise_test_support::binary_fixture("");
        let source = format!("{functions}\nfun entry(): Unit {{\n{checks}\n}}\n");
        let run = emit_link_and_run("bitwise.ko", &source, "entry");
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, expected.as_bytes());
        assert!(run.stderr.is_empty(), "{run:?}");
    }

    #[test]
    fn bitwise_inv_native_boundaries_and_single_evaluation() {
        let (functions, checks, expected) = crate::bitwise_test_support::inv_fixture("");
        let source = format!("{functions}\nfun entry(): Unit {{\n{checks}\n}}\n");
        let run = emit_link_and_run("integer-inv.ko", &source, "entry");
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, expected.as_bytes());
        assert!(run.stderr.is_empty(), "{run:?}");
    }

    #[test]
    fn bitwise_inv_native_literal_chains_and_last_use_fields() {
        let source = format!(
            "{}\nfun entry(): Unit {{ exercise() }}",
            crate::bitwise_test_support::inv_receiver_fixture()
        );
        let run = emit_link_and_run("inv-receivers.ko", &source, "entry");
        assert!(run.status.success(), "{run:?}");
        assert_eq!(
            run.stdout,
            crate::bitwise_test_support::INV_RECEIVER_STDOUT.as_bytes()
        );
        assert!(run.stderr.is_empty(), "{run:?}");
    }

    #[test]
    fn bitwise_native_guide_litmus_12_runs_the_normative_source() {
        let run = emit_link_and_run(
            "guide-litmus-12.ko",
            crate::bitwise_test_support::guide_litmus_12(),
            "main",
        );
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, b"Mask initialized\n");
        assert!(run.stderr.is_empty(), "{run:?}");
    }

    #[test]
    fn bitwise_native_operands_preserve_control_exit_and_order() {
        let (functions, checks, expected) = crate::bitwise_test_support::control_fixture();
        let source = format!("{functions}\nfun entry(): Unit {{ {checks} }}");
        let run = emit_link_and_run("bitwise-control.ko", &source, "entry");
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, expected.as_bytes());
        assert!(run.stderr.is_empty(), "{run:?}");
    }

    #[test]
    fn bitwise_native_const_and_runtime_agree_with_independent_oracles() {
        let (functions, checks, expected) =
            crate::bitwise_test_support::constant_runtime_fixture("");
        let source = format!("{functions}\nfun entry(): Unit {{\n{checks}\n}}");
        let run = emit_link_and_run("const-runtime-bitwise.ko", &source, "entry");
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, expected.as_bytes());
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

pub(crate) mod resource_exchange_tests {
    //! Resource lexical cleanup composed with owned whole-root replace/swap.

    use super::super::{SymbolKind, UnitSource, emit_link_and_run};

    const RESOURCES: &str = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
class Guard(val name: String, val first: Leaf, val second: Leaf) {
    deinit() { println(this.name) }
}
fun replacement(): Guard {
    println("evaluate new")
    return Guard("new body", Leaf("new first"), Leaf("new second"))
}
fun consume(own guard: Guard): Unit { println("consume old") }
"#;

    #[test]
    fn resource_exchange_native_commit_keeps_owners_until_lexical_cleanup() {
        let source = format!(
            r#"{RESOURCES}
fun entry(): Unit {{
    var first = Guard("old body", Leaf("old first"), Leaf("old second"))
    var second = Guard("second body", Leaf("second first"), Leaf("second second"))
    val old = replace(&first, replacement())
    println("after replace")
    consume(old)
    swap(&first, &second)
    println("after swap")
}}
"#
        );
        let expected = b"evaluate new\nafter replace\nconsume old\nold body\nold second\nold first\nafter swap\nnew body\nnew second\nnew first\nsecond body\nsecond second\nsecond first\n";
        let run = emit_link_and_run("resource-exchange.ko", &source, "entry");
        super::super::boxed_enum_tests::assert_success(&run, expected);
        let analysis = UnitSource::analyze("resource-exchange-counted.ko", &source)
            .expect("original source must pass strict Unit admission");
        analysis.assert_frontend_clean();
        let (program, entry) = analysis
            .lower_entry(analysis.declaration("entry", SymbolKind::Function))
            .expect("resource exchange lowers without introducing an early drop");
        let ir = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        // Three guards each own two leaves. Exchange transfers each allocation unchanged.
        let counted = super::super::boxed_enum_tests::run_counted_allocations(&ir, 9);
        super::super::boxed_enum_tests::assert_success(&counted, expected);
    }

    #[test]
    fn resource_exchange_native_replace_return_and_borrowed_result_drop_once() {
        let source = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
fun takeOld(): Leaf {
    var target = Leaf("new root")
    return replace(&target, Leaf("replacement root"))
}
fun inspect(leaf: Leaf): Unit { println("inspect old") }
fun entry(): Unit {
    val returned = takeOld()
    println("received old")
    var target = Leaf("borrowed old")
    inspect(replace(&target, Leaf("borrowed replacement")))
    println("after inspect")
}
"#;
        let run = emit_link_and_run("resource-exchange-return.ko", source, "entry");
        super::super::boxed_enum_tests::assert_success(
        &run,
        b"replacement root\nreceived old\ninspect old\nborrowed old\nafter inspect\nborrowed replacement\nnew root\n",
    );
    }

    #[test]
    fn resource_exchange_native_pending_control_exit_retains_old_root() {
        for exit in ["return", "break", "continue"] {
            let source = format!(
                r#"
class Leaf(val name: String) {{ deinit() {{ println(this.name) }} }}
fun exercise(flag: Boolean): Unit {{
    var target = Leaf("old")
    var once = true
    loop {{
        if (!once) {{ break }}
        once = false
        val old = replace(&target, if (flag) {{ {exit} }} else {{ Leaf("new") }})
        println("committed")
        break
    }}
    println("after loop")
}}
fun entry(): Unit {{ exercise(true); exercise(false); println("done") }}
"#
            );
            let run = emit_link_and_run("resource-exchange-exit.ko", &source, "entry");
            let expected = if exit == "return" {
                "old\ncommitted\nold\nafter loop\nnew\ndone\n"
            } else {
                "after loop\nold\ncommitted\nold\nafter loop\nnew\ndone\n"
            };
            super::super::boxed_enum_tests::assert_success(&run, expected.as_bytes());
        }
    }

    #[test]
    fn resource_exchange_native_abort_does_not_commit_or_run_resource_cleanup() {
        let source = r#"
class Leaf { deinit() { println("unexpected deinit") } }
fun entry(): Unit {
    var target = Leaf()
    println("before abort")
    val old = replace(&target, error("stop"))
    println("unexpected commit")
}
"#;
        let run = emit_link_and_run("resource-exchange-abort.ko", source, "entry");
        assert!(!run.status.success(), "{run:?}");
        assert_eq!(run.stdout, b"before abort\n");
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(run.status.signal(), Some(6), "{run:?}");
        }
    }
}

pub(crate) mod field_resource_replace_tests {
    //! Direct field exchange composed with ordinary concrete resource class cleanup.

    use super::super::{SymbolKind, UnitSource, emit_link_and_run};

    const RESOURCES: &str = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
class Guard(val name: String, val first: Leaf, val second: Leaf) {
    deinit() { println(this.name) }
}
class Holder(var state: Guard, val sibling: Leaf) {
    deinit() { println("parent body") }
}
fun replacement(): Guard {
    println("evaluate new")
    return Guard("new body", Leaf("new first"), Leaf("new second"))
}
fun inspect(guard: Guard): Unit { println("inspect old") }
"#;

    #[test]
    fn field_resource_replace_native_commit_preserves_parent_and_lexical_owners() {
        let source = format!(
            r#"{RESOURCES}
fun entry(): Unit {{
    val holder = Holder(Guard("old body", Leaf("old first"), Leaf("old second")), Leaf("sibling"))
    val old = replace(&holder.state, replacement())
    println("after replace")
    inspect(old)
    println("after inspect")
}}
"#
        );
        let expected = b"evaluate new\nafter replace\ninspect old\nafter inspect\nold body\nold second\nold first\nparent body\nsibling\nnew body\nnew second\nnew first\n";
        assert_run_and_counted(&source, 8, expected);
    }

    #[test]
    fn field_resource_replace_native_return_and_borrow_transfer_old_once() {
        let source = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
class Holder(var state: Leaf)
fun takeOld(): Leaf {
    val holder = Holder(Leaf("returned old"))
    return replace(&holder.state, Leaf("returned replacement"))
}
fun inspect(leaf: Leaf): Unit { println("inspect old") }
fun entry(): Unit {
    val returned = takeOld()
    println("received old")
    val holder = Holder(Leaf("borrowed old"))
    inspect(replace(&holder.state, Leaf("borrowed replacement")))
    println("after inspect")
}
"#;
        assert_run_and_counted(
        source,
        6,
        b"returned replacement\nreceived old\ninspect old\nborrowed old\nafter inspect\nborrowed replacement\nreturned old\n",
    );
    }

    #[test]
    fn field_resource_replace_native_pending_control_exit_keeps_old_field() {
        for exit in ["return", "break", "continue"] {
            let source = format!(
                r#"
class Leaf(val name: String) {{ deinit() {{ println(this.name) }} }}
class Holder(var state: Leaf) {{ deinit() {{ println("parent body") }} }}
fun exercise(flag: Boolean): Unit {{
    val holder = Holder(Leaf("old"))
    var once = true
    loop {{
        if (!once) {{ break }}
        once = false
        val old = replace(&holder.state, if (flag) {{ {exit} }} else {{ Leaf("new") }})
        println("committed")
        break
    }}
    println("after loop")
}}
fun entry(): Unit {{ exercise(true); exercise(false); println("done") }}
"#
            );
            let expected = if exit == "return" {
                "parent body\nold\ncommitted\nold\nafter loop\nparent body\nnew\ndone\n"
            } else {
                "after loop\nparent body\nold\ncommitted\nold\nafter loop\nparent body\nnew\ndone\n"
            };
            assert_run_and_counted(&source, 5, expected.as_bytes());
        }
    }

    #[test]
    fn field_resource_replace_native_abort_does_not_commit_or_run_deinit() {
        let source = r#"
class Leaf { deinit() { println("unexpected leaf deinit") } }
class Holder(var state: Leaf) { deinit() { println("unexpected parent deinit") } }
fun entry(): Unit {
    val holder = Holder(Leaf())
    println("before abort")
    val old = replace(&holder.state, error("stop"))
    println("unexpected commit")
}
"#;
        let run = emit_link_and_run("field-resource-abort.ko", source, "entry");
        assert!(!run.status.success(), "{run:?}");
        assert_eq!(run.stdout, b"before abort\n");
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(run.status.signal(), Some(6), "{run:?}");
        }
    }

    fn assert_run_and_counted(source: &str, allocations: usize, expected: &[u8]) {
        let run = emit_link_and_run("field-resource-replace.ko", source, "entry");
        super::super::boxed_enum_tests::assert_success(&run, expected);
        let analysis = UnitSource::analyze("field-resource-counted.ko", source)
            .expect("original source must pass strict Unit admission");
        analysis.assert_frontend_clean();
        let (program, entry) = analysis
            .lower_entry(analysis.declaration("entry", SymbolKind::Function))
            .expect("field resource replacement lowers to verified SSA");
        let ir = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let counted = super::super::boxed_enum_tests::run_counted_allocations(&ir, allocations);
        super::super::boxed_enum_tests::assert_success(&counted, expected);
    }
}

pub(crate) mod map_sibling_route_tests {
    //! Unreachable typed Map iteration must not affect lowering a reachable callable.

    use super::super::emit_link_and_run;

    const UNUSED_MAP_ITERATION: &str = r#"
fun unrelated(map: Map<Int, Int>): Unit {
    for ((key, value) in map) { val unused = value }
}
"#;

    fn check(source: &str, stdout: &[u8]) {
        // Exercise the historical success case before the otherwise identical source.
        for (path, suffix) in [
            ("with-map-sibling.ko", UNUSED_MAP_ITERATION),
            ("without-map-sibling.ko", ""),
        ] {
            let run = emit_link_and_run(path, &format!("{source}{suffix}"), "main");
            assert!(run.status.success(), "{path}: {run:?}");
            assert_eq!(run.stdout, stdout, "{path}");
            assert!(run.stderr.is_empty(), "{path}: {run:?}");
        }
    }

    #[test]
    fn numeric_range_with_nullable_borrow_parameter_is_independent_of_map_sibling() {
        check(
            r#"
fun scan(s: String?, r: IntRange): Unit {
    for (i in r) { if (i == 1) { println("one") } else { println("two") } }
}
public fun main(): Unit { val none: String? = null; scan(none, 1..2) }
"#,
            b"one\ntwo\n",
        );
    }

    #[test]
    fn map_consume_with_nullable_borrow_parameter_is_independent_of_map_sibling() {
        check(
            r#"
fun scan(s: String?): Unit {
    var map = mutableMapOf<Int, Int>()
    map.put(1, 2)
    val frozen = map.consume()
    if (frozen.size == 1) { println("size") }
}
public fun main(): Unit { val none: String? = null; scan(none) }
"#,
            b"size\n",
        );
    }

    #[test]
    fn delegated_receiver_with_nullable_borrow_parameter_is_independent_of_map_sibling() {
        check(
            r#"
interface Read { fun value(): Int }
class Reader: Read { override fun value(): Int = 7 }
class Wrapper(val reader: Reader): Read by reader
fun scan(s: String?, wrapper: Wrapper): Unit {
    if (wrapper.value() == 7) { println("delegate") }
}
public fun main(): Unit { val none: String? = null; scan(none, Wrapper(Reader())) }
"#,
            b"delegate\n",
        );
    }

    #[test]
    fn numeric_range_with_loop_reassigns_nullable_owner_is_independent_of_map_sibling() {
        check(
            r#"
fun scan(r: IntRange): Unit {
    var outer: String? = "first"
    if (outer != null) {
        var i = 0
        while (i < 2) {
            outer = "second"
            break
        }
    }
    for (i in r) { if (i == 1) { println("one") } else { println("two") } }
}
public fun main(): Unit { scan(1..2) }
"#,
            b"one\ntwo\n",
        );
    }

    #[test]
    fn map_consume_with_loop_reassigns_nullable_owner_is_independent_of_map_sibling() {
        check(
            r#"
fun scan(): Unit {
    var outer: String? = "first"
    if (outer != null) {
        var i = 0
        while (i < 2) {
            outer = "second"
            break
        }
    }
    var map = mutableMapOf<Int, Int>()
    map.put(1, 2)
    val frozen = map.consume()
    if (frozen.size == 1) { println("size") }
}
public fun main(): Unit { scan() }
"#,
            b"size\n",
        );
    }

    #[test]
    fn delegated_receiver_with_loop_reassigns_nullable_owner_is_independent_of_map_sibling() {
        check(
            r#"
interface Read { fun value(): Int }
class Reader: Read { override fun value(): Int = 7 }
class Wrapper(val reader: Reader): Read by reader
fun scan(wrapper: Wrapper): Unit {
    var outer: String? = "first"
    if (outer != null) {
        var i = 0
        while (i < 2) {
            outer = "second"
            break
        }
    }
    if (wrapper.value() == 7) { println("delegate") }
}
public fun main(): Unit { scan(Wrapper(Reader())) }
"#,
            b"delegate\n",
        );
    }
}

pub(crate) mod non_null_assertion_tests {
    use super::super::emit_link_and_run;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn non_null_assertion_pointer_results_and_shadowed_error_run_natively() {
        for (inner, constructor, payload) in [
            ("Node", "Node(37)", "node.item"),
            ("Box<Token>", "Box(Token(37))", ""),
            ("Rc<Int>", "Rc(37)", "node.value"),
        ] {
            let observe = if payload.is_empty() {
                // Box payload projections remain deferred; accepting its Value parameter tests delivery.
                "println(\"ok\")".to_owned()
            } else {
                format!("if ({payload} == 37) {{ println(\"ok\") }} else {{ println(\"bad\") }}")
            };
            let declarations = format!(
                r#"
class Node(val item: Int)
value class Token(val item: Int)
fun error(message: String): Int {{ val printed = println("shadow")
return 7 }}
fun make(): {inner}? {{ val printed = println("make")
val result: {inner}? = {constructor}
return result }}
fun observe(own node: {inner}): Unit {{ {observe} }}
"#
            );
            let source = format!(
                r#"{declarations}
fun entry(): Unit {{ val explicit = error("ordinary")
val checked = if (explicit == 7) {{ observe(make()!!) }} else {{ println("wrong target") }} }}
"#
            );
            let run = emit_link_and_run("assertion-present.ko", &source, "entry");
            assert!(run.status.success(), "{inner}: {run:?}");
            assert_eq!(run.stdout, b"shadow\nmake\nok\n", "{inner}");
            assert!(run.stderr.is_empty(), "{inner}: {run:?}");
            let absent = format!(
                r#"{declarations}
fun entry(): Unit {{ val node: {inner}? = null
val checked = observe(node!!) }}
"#
            );
            let run = emit_link_and_run("assertion-null.ko", &absent, "entry");
            assert_eq!(
                run.status.signal(),
                Some(6),
                "{inner}: assertion must SIGABRT despite shadowed error: {run:?}"
            );
        }
    }

    #[test]
    fn non_null_assertion_transfers_one_allocation_and_preserves_explicit_rc_alias() {
        use super::super::{TestDirectory, UnitSource};
        use lang_frontend::name_resolution::SymbolKind;
        use std::{fs, process::Command};

        for (inner, constructor, payload) in [
            ("Node", "Node(37)", "node.item"),
            ("Box<Token>", "Box(Token(37))", ""),
            ("Rc<Int>", "Rc(37)", "node.value"),
        ] {
            let shared = usize::from(inner == "Rc<Int>");
            let keep = if shared == 1 {
                "val alias = node.share()\nreturn alias"
            } else {
                "return node"
            };
            let observe = if payload.is_empty() {
                "println(\"ok\")".to_owned()
            } else {
                format!("if ({payload} == 37) {{ println(\"ok\") }} else {{ println(\"bad\") }}")
            };
            let source = format!(
                r#"
class Node(val item: Int)
value class Token(val item: Int)
fun make(): {inner}? {{ val printed = println("make")
val result: {inner}? = {constructor}
return result }}
fun keep(own node: {inner}): {inner} {{ {keep} }}
fun observe(own node: {inner}): Unit {{ {observe} }}
fun entry(): Unit {{ val checked = observe(keep(make()!!)) }}
"#
            );
            let analysis = UnitSource::analyze("assertion-counted.ko", &source)
                .expect("original source must pass strict Unit admission");
            analysis.assert_frontend_clean();
            let (program, entry) = analysis
                .lower_entry(analysis.declaration("entry", SymbolKind::Function))
                .expect("counted assertion source must verify");
            let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
                .expect("counted assertion LLVM must verify")
                .replace("@malloc(", "@counted_malloc(")
                .replace("@free(", "@counted_free(");
            let mut instrumented = String::new();
            let mut retain_sites = 0;
            let mut release_sites = 0;
            for line in llvm.lines() {
                instrumented.push_str(line);
                instrumented.push('\n');
                if line.contains(".next = add i64") {
                    retain_sites += 1;
                    instrumented.push_str("  call void @counted_retain()\n");
                } else if line.contains("%strong.next = sub i64") {
                    release_sites += 1;
                    instrumented.push_str("  call void @counted_release()\n");
                }
            }
            assert_eq!((retain_sites, release_sites), (shared, shared));
            instrumented
                .push_str("declare void @counted_retain()\ndeclare void @counted_release()\n");
            let directory = TestDirectory::create();
            let ir = directory.join("assertion.ll");
            let counter = directory.join("counter.c");
            let executable = directory.join("counted-assertion");
            fs::write(&ir, instrumented).expect("write instrumented LLVM");
            fs::write(
                &counter,
                r#"
#include <stdlib.h>
#include <assert.h>
static void *live;
static int allocations, releases, retains, shared_releases;
void counted_retain(void) { ++retains; }
void counted_release(void) { ++shared_releases; }
void *counted_malloc(size_t size) {
    assert(allocations++ == 0);
    live = malloc(size);
    assert(live);
    return live;
}
void counted_free(void *value) {
    assert(value == live && releases++ == 0);
    free(value);
    live = 0;
}
__attribute__((destructor)) static void verify_counts(void) {
    assert(allocations == 1 && releases == 1 && !live);
    assert(retains == EXPECT_SHARED);
    assert(shared_releases == 2 * EXPECT_SHARED);
}
"#,
            )
            .expect("write dynamic ownership counter");
            let linked = Command::new(crate::test_support::ir_clang())
                .arg(&ir)
                .arg(&counter)
                .arg(format!("-DEXPECT_SHARED={shared}"))
                .arg("-o")
                .arg(&executable)
                .output()
                .expect("clang must launch");
            assert!(linked.status.success(), "{inner}: {linked:?}");
            let run = Command::new(&executable)
                .output()
                .expect("counted program must launch");
            assert!(run.status.success(), "{inner}: {run:?}");
            assert_eq!(run.stdout, b"make\nok\n", "{inner}");
            assert!(run.stderr.is_empty(), "{inner}: {run:?}");
        }
    }
}
