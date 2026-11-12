//! 普通源码的借用结果 ABI，不做故障或 IR 注入。
use super::*;

#[test]
fn native_borrow_last_use_single_preserves_permissions_and_owner_cleanup() {
    check_normal_cases(crate::native_borrow_last_use_cases::cases());
}

#[test]
fn native_borrow_direct_alias_single_preserves_real_storage_and_owner_cleanup() {
    check_normal_cases(crate::native_borrow_last_use_cases::alias_cases());
}

#[test]
fn native_borrow_stable_place_single_preserves_storage_and_cleanup() {
    check_normal_cases(crate::native_borrow_place_cases::cases());
}

#[test]
fn native_nullable_map_promotion_single_transfers_and_drops_generic_payloads() {
    check_normal_cases(crate::native_map_promotion_cases::cases());
}

#[test]
fn native_borrowed_inline_nullable_presence_preserves_empty_string_and_storage() {
    let path = "borrowed-inline-nullable-presence.ko";
    let source = r#"
fun inspect(value: String?): Unit {
    if (value != null) { println("present") } else { println("absent") }
    if (null == value) { println("equal") } else { println("different") }
}
fun entry(): Unit {
    inspect(null)
    inspect("")
    val text: String? = "owned"
    inspect(text)
}
"#;
    let run = emit_link_and_run(path, source, "entry");
    super::boxed_enum_tests::assert_success(
        &run,
        b"absent\nequal\npresent\ndifferent\npresent\ndifferent\n",
    );
    assert_presence_field_reads(path, source, "ptr", 2);
}

#[test]
fn native_borrowed_inline_nullable_presence_reads_the_tag_of_a_moveonly_value() {
    let path = "borrowed-tagged-nullable-presence.ko";
    let source = r#"
value class Payload(val text: String)
fun inspect(value: Payload?): Unit {
    if (value != null) { println("present") } else { println("absent") }
}
fun entry(): Unit {
    inspect(null)
    inspect(Payload("text"))
}
"#;
    let run = emit_link_and_run(path, source, "entry");
    super::boxed_enum_tests::assert_success(&run, b"absent\npresent\n");
    assert_presence_field_reads(path, source, "i1", 1);
}

fn assert_presence_field_reads(path: &str, source: &str, ty: &str, reads: usize) {
    let ir = super::boxed_enum_tests::lower_to_llvm(path, source);
    let signature = ir
        .lines()
        .find(|line| line.starts_with("define ") && line.contains(".inspect."))
        .expect("definition for the source inspect function");
    let (_, tail) = ir.split_once(signature).expect("inspect definition start");
    let (body, _) = tail.split_once("\n}").expect("inspect definition end");
    // Printing loads local literal temporaries; ignore those exact alloca slots.
    let local_slots = body
        .lines()
        .filter_map(|line| line.trim().split_once(" = alloca ").map(|(slot, _)| slot))
        .collect::<Vec<_>>();
    let loan_loads = body
        .lines()
        .filter_map(|line| line.split_once(" = load ").map(|(_, load)| load))
        .filter(|load| {
            let (_, address) = load.split_once(", ptr ").expect("load source address");
            let (address, _) = address.split_once(", ").expect("load source operand");
            !local_slots.contains(&address)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        loan_loads.len(),
        reads,
        "inspect loads only presence: {body}"
    );
    let expected = format!("{ty}, ptr %nullable.borrowed.presence.slot");
    assert!(
        loan_loads.iter().all(|load| load.starts_with(&expected)),
        "{body}"
    );
    let slots = body
        .lines()
        .filter(|line| line.contains(" = getelementptr "))
        .collect::<Vec<_>>();
    assert_eq!(slots.len(), reads, "{body}");
    assert!(
        slots.iter().all(|slot| slot.ends_with(", i32 0, i32 0")),
        "{body}"
    );
}

fn check_normal_cases(cases: impl IntoIterator<Item = crate::native_borrow_last_use_cases::Case>) {
    for case in cases {
        let run = emit_link_and_run(case.name, case.source, "entry");
        super::boxed_enum_tests::assert_success(&run, case.stdout);
        let llvm = super::boxed_enum_tests::lower_to_llvm(case.name, case.source);
        let counted = super::boxed_enum_tests::run_counted_allocations(&llvm, case.allocations);
        super::boxed_enum_tests::assert_success(&counted, case.stdout);
    }
}

#[test]
fn native_borrow_storage_single_generic_projection_and_nullable() {
    for callable in ["view", "wrap"] {
        for (name, text, expected) in crate::native_borrow_storage_cases::cases(callable) {
            let run = emit_link_and_run(name, &text, "entry");
            assert!(run.status.success(), "{name}/{callable}: {run:?}");
            assert_eq!(run.stdout, expected, "{name}/{callable}");
        }
    }
}

#[test]
fn native_borrow_result_single_direct_and_wrapper_keep_source_until_scope_end() {
    for callable in ["view", "wrap"] {
        let text = format!(
            "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)\nfun consume(own source: String) {{ println(source) }}\nfun entry() {{ val source = \"kept\".clone(); {{ borrow val item = {callable}(source); println((item)) }}; consume(source) }}"
        );
        let run = emit_link_and_run("borrow_result.ko", &text, "entry");
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, b"kept\nkept\n");
    }
}

#[test]
fn native_borrow_result_single_scalar_pointer_reads_copyable_storage() {
    let text = "fun view(source: Int): borrow Int from source = source\nfun wrap(source: Int): borrow Int from source = view(source)\nfun observe(own source: Int) { if (source == 7) { println(\"seven\") } else { error(\"wrong value\") } }\nfun entry() { val source = 7; { borrow val item = wrap(source); observe(item) }; observe(source) }";
    let run = emit_link_and_run("borrow_int.ko", text, "entry");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"seven\nseven\n");
}

#[test]
fn native_map_require_single_borrows_storage_and_aborts_on_missing() {
    for (name, text, expected) in crate::native_map_require_cases::cases() {
        let run = emit_link_and_run(name, text, "entry");
        assert!(run.status.success(), "{name}: {run:?}");
        assert_eq!(run.stdout, expected, "{name}");
    }
    let run = emit_link_and_run(
        "require-missing",
        crate::native_map_require_cases::MISSING,
        "entry",
    );
    assert!(!run.status.success(), "a missing required key must abort");
}

#[test]
fn native_map_with_single_synchronously_borrows_slot_and_handles_missing() {
    let text = "fun entry() { var m = mutableMapOf<String, String>(); m.put(\"key\", \"value\".clone()); if (m.withValue(\"key\", { value -> println(value); return })) { println(\"found\") }; if (m.withValue(\"missing\", { value -> error(\"must not call\") })) { error(\"must be false\") }; m.put(\"next\", \"after\") }";
    let run = emit_link_and_run("map-with.ko", text, "entry");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"value\nfound\n");
}

#[test]
fn native_map_with_single_generic_matrix() {
    for (name, text, expected) in crate::native_map_with_cases::cases() {
        let run = emit_link_and_run(name, &text, "entry");
        assert!(run.status.success(), "{name}: {run:?}");
        assert_eq!(run.stdout, expected, "{name}");
    }
}

#[test]
fn native_map_with_single_key_and_capture_owners_release_exactly_once() {
    for (name, text, expected) in crate::native_map_with_cases::cases()
        .into_iter()
        .filter(|(name, _, _)| matches!(*name, "capture" | "resource"))
    {
        // capture: buffer + 7 String owners; resource: buffer + 3 keys + 2 Resource owners.
        let count = if name == "capture" { 8 } else { 6 };
        let llvm = super::boxed_enum_tests::lower_to_llvm(name, &text);
        let run = super::boxed_enum_tests::run_counted_allocations(&llvm, count);
        assert!(run.status.success(), "{name}: {run:?}");
        assert_eq!(run.stdout, expected, "{name}");
    }
}

#[test]
fn borrow_call_lowering_single_groups_and_field_forwarding_run_without_owner_leaks() {
    for (name, provider, body, allocations, expected) in [
        (
            "grouped-binding",
            "fun view(source: String): borrow String from source = source",
            "val source = \"kept\".clone(); borrow val item = ((view(source))); println(item)",
            1,
            &b"kept\n"[..],
        ),
        (
            "grouped-return",
            "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = ((view(source)))",
            "val source = \"kept\".clone(); borrow val item = wrap(source); println(item)",
            1,
            &b"kept\n"[..],
        ),
        (
            "field-forwarding",
            "class Packet(val text: String)\nfun view(source: String): borrow String from source = source\nfun wrap(source: Packet): borrow String from source = view(source.text)\nfun consume(own source: Packet) {}",
            "val source = Packet(\"kept\".clone()); borrow val item = wrap(source); println(item); consume(source)",
            2,
            &b"kept\n"[..],
        ),
    ] {
        let text = format!("{provider}\nfun entry() {{ {body} }}");
        let run = emit_link_and_run(name, &text, "entry");
        super::boxed_enum_tests::assert_success(&run, expected);
        let llvm = super::boxed_enum_tests::lower_to_llvm(name, &text);
        let counted = super::boxed_enum_tests::run_counted_allocations(&llvm, allocations);
        super::boxed_enum_tests::assert_success(&counted, expected);
    }
}

#[test]
fn multi_parameter_borrow_single_native_matrix() {
    for case in crate::native_multi_borrow_cases::cases() {
        let run = emit_link_and_run(case.name, case.source, "entry");
        assert!(run.status.success(), "{}: {run:?}", case.name);
        assert_eq!(run.stdout, case.stdout, "{}", case.name);
        assert!(run.stderr.is_empty(), "{}: {run:?}", case.name);
    }
}

#[test]
fn mixed_value_borrow_single_native_matrix() {
    for case in crate::native_mixed_borrow_cases::cases()
        .into_iter()
        .chain([crate::native_mixed_borrow_cases::generic_consumers()])
    {
        let run = emit_link_and_run(case.name, case.source, "entry");
        assert!(run.status.success(), "{}: {run:?}", case.name);
        assert_eq!(run.stdout, case.stdout, "{}", case.name);
        assert!(run.stderr.is_empty(), "{}: {run:?}", case.name);
    }
}

#[test]
fn nested_borrow_argument_single_native_matrix() {
    for case in crate::native_nested_borrow_cases::cases() {
        let run = emit_link_and_run(case.name, case.source, "entry");
        assert!(run.status.success(), "{}: {run:?}", case.name);
        assert_eq!(run.stdout, case.stdout, "{}", case.name);
        assert!(run.stderr.is_empty(), "{}: {run:?}", case.name);
    }
}

#[test]
fn borrow_value_field_single_native_matrix() {
    for (name, source, stdout) in crate::native_borrow_value_field_cases::cases() {
        let run = emit_link_and_run(name, source, "entry");
        assert!(run.status.success(), "{name}: {run:?}");
        assert_eq!(run.stdout, stdout, "{name}");
        assert!(run.stderr.is_empty(), "{name}: {run:?}");
    }
}

#[test]
fn native_loop_local_no_carry_false_edge_keeps_external_source() {
    let text = r#"fun view(source: String): borrow String from source = source
fun observe(item: String): Unit { println("unexpected-body") }
fun run(source: String, flag: Boolean): Unit {
 while(flag) { borrow val item = view(source); observe(item) }
}
fun take(own source: String): Unit { println(source) }
fun entry() {
 val source = "kept".clone()
 run(source, false)
 take(source)
}"#;
    let run = emit_link_and_run("loop-local-false", text, "entry");
    super::boxed_enum_tests::assert_success(&run, b"kept\n");
}
