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

#[test]
fn native_no_carry_sibling_false_edges_keep_the_same_source() {
    let text = r#"fun view(source: String): borrow String from source = source
fun observe(item: String): Unit { println("unexpected-body") }
fun run(source: String, flag: Boolean): Unit {
 while(flag) { borrow val first = view(source); observe(first) }
 while(flag) { borrow val second = view(source); observe(second); observe(second) }
}
fun take(own source: String): Unit { println(source) }
fun entry() {
 val source = "kept".clone()
 run(source, false)
 take(source)
}"#;
    let run = emit_link_and_run("no-carry-sibling-false", text, "entry");
    super::boxed_enum_tests::assert_success(&run, b"kept\n");
}

#[test]
fn native_no_carry_distinct_flags_false_edges_keep_source() {
    let text = r#"fun view(source: String): borrow String from source = source
fun observe(item: String): Unit { println("unexpected-body") }
fun run(flag1: Boolean, source: String, flag0: Boolean): Unit {
 while(flag0) { borrow val first = view(source); observe(first) }
 while(flag1) { borrow val second = view(source); observe(second); observe(second) }
 while(flag0) { borrow val third = view(source); observe(third) }
}
fun take(own source: String): Unit { println(source) }
fun entry() {
 val source = "kept".clone()
 run(false, source, false)
 take(source)
}"#;
    let run = emit_link_and_run("no-carry-distinct-flags-false", text, "entry");
    super::boxed_enum_tests::assert_success(&run, b"kept\n");
}

#[test]
fn native_no_carry_mixed_own_flags_false_edges_keep_source() {
    let text = r#"fun view(source: String): borrow String from source = source
fun observe(item: String): Unit { println("unexpected-body") }
fun run(flag1: Boolean, source: String, own flag0: Boolean): Unit {
 while(flag0) { borrow val first = view(source); observe(first) }
 while(flag1) { borrow val second = view(source); observe(second); observe(second) }
 while(flag0) { borrow val third = view(source); observe(third) }
}
fun take(own source: String): Unit { println(source) }
fun entry() {
 val source = "kept".clone()
 run(false, source, false)
 take(source)
}"#;
    let run = emit_link_and_run("no-carry-mixed-own-flags-false", text, "entry");
    super::boxed_enum_tests::assert_success(&run, b"kept\n");
}

#[test]
fn native_no_carry_source_segments_survive_false_edges() {
    let text = r#"fun view(source: String): borrow String from source = source
fun observe(item: String): Unit { println("unexpected-body") }
fun before(source: String): Unit { println("pre"); println(source) }
fun between(source: String): Unit { println("mid"); println(source) }
fun after(source: String): Unit { println("tail"); println(source) }
fun run(flag1: Boolean, source: String, own flag0: Boolean): Unit {
 before(source)
 while(flag0) { borrow val first = view(source); observe(first) }
 between(source)
 while(flag1) { borrow val second = view(source); observe(second) }
 after(source)
}
fun take(own source: String): Unit { println(source) }
fun entry() {
 val source = "kept".clone()
 run(false, source, false)
 take(source)
}"#;
    let run = emit_link_and_run("no-carry-source-segments-false", text, "entry");
    super::boxed_enum_tests::assert_success(&run, b"pre\nkept\nmid\nkept\ntail\nkept\nkept\n");
}

#[test]
fn native_loop_arm_no_carry_zero_iteration_preserves_source() {
    // Selected while condition is a stable Boolean parameter. Passing true would
    // not return normally; do not disguise timeout/abort as finite positive coverage.
    let text = r#"fun view(source:String):borrow String from source=source
fun consume(item:String):Unit { println("unexpected") }
fun run(source:String,flag:Boolean,choose:Boolean):Unit {
 while(flag) { if(choose) { borrow val a=view(source); consume(a) }
 else { borrow val b=view(source); consume(b); consume(b) } }
}
fun take(own source:String):Unit { println(source) }
fun entry() { val source="kept".clone(); run(source,false,true); run(source,false,false); take(source) }
"#;
    let run = emit_link_and_run("loop-arm-zero", text, "entry");
    super::boxed_enum_tests::assert_success(&run, b"kept\n");
}

#[test]
fn native_counter_loop_arms_run_zero_one_two_then_return_source() {
    let text = r#"fun view(source:String):borrow String from source=source
fun left(item:String):Unit {println("left");println(item)}
fun right(item:String):Unit {println("right");println(item)}
fun run(source:String,rounds:Int,choose:Boolean):Unit {
 var remaining=rounds
 while(remaining>0) {
  if(choose){borrow val a=view(source);left(a)}
  else {borrow val b=view(source);right(b);right(b)}
  remaining=remaining-1
 }
}
fun take(own source:String):Unit {println(source)}
fun entry() {
 val source="kept".clone()
 run(source,0,true);println("returned-0-left");println(source)
 run(source,0,false);println("returned-0-right");println(source)
 run(source,1,true);println("returned-1-left");println(source)
 run(source,1,false);println("returned-1-right");println(source)
 run(source,2,true);println("returned-2-left");println(source)
 run(source,2,false);println("returned-2-right");println(source)
 take(source)
}"#;
    let run = emit_link_and_run("counter-loop-arms-finite", text, "entry");
    super::boxed_enum_tests::assert_success(&run,b"returned-0-left\nkept\nreturned-0-right\nkept\nleft\nkept\nreturned-1-left\nkept\nright\nkept\nright\nkept\nreturned-1-right\nkept\nleft\nkept\nleft\nkept\nreturned-2-left\nkept\nright\nkept\nright\nkept\nright\nkept\nright\nkept\nreturned-2-right\nkept\nkept\n");
}

#[test]
fn native_counter_loop_reordered_own_parameters_preserve_source() {
    let text = r#"fun view(source:String):borrow String from source=source
fun left(item:String):Unit {println("L");println(item)}
fun right(item:String):Unit {println("R");println(item)}
fun run(own choose:Boolean,own rounds:Int,source:String):Unit {
 var remaining=rounds
 while(remaining>0) {
  if(choose){borrow val a=view(source);left(a)}
  else {borrow val b=view(source);right(b);right(b)}
  remaining=remaining-1
 }
}
fun take(own source:String):Unit {println(source)}
fun entry() {
 val source="kept".clone()
 run(true,-1,source);run(false,-1,source);println("negative-returned");println(source)
 run(true,0,source);run(false,0,source);println("zero-returned");println(source)
 run(true,1,source);run(false,1,source);println("one-returned");println(source)
 run(true,2,source);run(false,2,source);println("two-returned");println(source)
 take(source)
}"#;
    let run = emit_link_and_run("counter-loop-own-reordered", text, "entry");
    super::boxed_enum_tests::assert_success(&run,b"negative-returned\nkept\nzero-returned\nkept\nL\nkept\nR\nkept\nR\nkept\none-returned\nkept\nL\nkept\nL\nkept\nR\nkept\nR\nkept\nR\nkept\nR\nkept\ntwo-returned\nkept\nkept\n");
}

#[test]
fn native_loop_arm_tail_break_runs_at_most_once_and_preserves_source() {
    let text = r#"fun view(source:String):borrow String from source=source
fun left(item:String):Unit {println("L");println(item)}
fun right(item:String):Unit {println("R");println(item)}
fun run(own choose:Boolean,source:String,own flag:Boolean):Unit {
 while(flag){if(choose){borrow val a=view(source);left(a)}else{borrow val b=view(source);right(b);right(b)};break}
 println("after-loop");println(source)
}
fun take(own source:String):Unit {println(source)}
fun entry(){val source="kept".clone();run(true,source,false);run(false,source,false);println("zero");println(source);run(true,source,true);println("left-exited");println(source);run(false,source,true);println("right-exited");println(source);take(source)}"#;
    let run = emit_link_and_run("arm-tail-break", text, "entry");
    super::boxed_enum_tests::assert_success(
        &run,
        b"after-loop\nkept\nafter-loop\nkept\nzero\nkept\nL\nkept\nafter-loop\nkept\nleft-exited\nkept\nR\nkept\nR\nkept\nafter-loop\nkept\nright-exited\nkept\nkept\n",
    );
}

#[test]
fn native_loop_arm_tail_break_after_scalar_prefix_cfg() {
    let text = r#"fun view(source:String):borrow String from source=source
fun left(item:String):Unit {println("L");println(item)}
fun right(item:String):Unit {println("R");println(item)}
fun tick(own value:Int):Unit {}
fun run(own choose:Boolean,source:String,own flag:Boolean):Unit {
 if(choose){tick(1)}else{tick(2)}
 while(flag){if(choose){borrow val a=view(source);left(a)}else{borrow val b=view(source);right(b);right(b)};break}
 println("after-loop");println(source)
}
fun take(own source:String):Unit {println(source)}
fun entry(){val source="kept".clone();run(true,source,false);run(false,source,false);println("zero");println(source);run(true,source,true);println("left-exited");println(source);run(false,source,true);println("right-exited");println(source);take(source)}"#;
    let run = emit_link_and_run("arm-tail-break-prefix", text, "entry");
    super::boxed_enum_tests::assert_success(
        &run,
        b"after-loop\nkept\nafter-loop\nkept\nzero\nkept\nL\nkept\nafter-loop\nkept\nleft-exited\nkept\nR\nkept\nR\nkept\nafter-loop\nkept\nright-exited\nkept\nkept\n",
    );
}

#[test]
fn native_loop_arm_tail_break_independent_prefix_parameter() {
    let mut text = String::from(
        r#"fun view(source:String):borrow String from source=source
fun left(item:String):Unit {println("L");println(item)}
fun right(item:String):Unit {println("R");println(item)}
fun tick(own value:Int):Unit {}
fun run(own flag:Boolean,own before:Boolean,source:String,own choose:Boolean):Unit {
 if(before){tick(1)}else{tick(2)}
 while(flag){if(choose){borrow val a=view(source);left(a)}else{borrow val b=view(source);right(b);right(b)};break}
 println("after-loop");println(source)
}
fun take(own source:String):Unit {println(source)}
fun entry(){val source="kept".clone();
"#,
    );
    let mut expected = String::new();
    for flag in [false, true] {
        for before in [false, true] {
            for choose in [false, true] {
                text.push_str(&format!("run({flag},{before},source,{choose});\n"));
                if flag {
                    expected.push_str(if choose {
                        "L\nkept\n"
                    } else {
                        "R\nkept\nR\nkept\n"
                    });
                }
                expected.push_str("after-loop\nkept\n");
            }
        }
    }
    text.push_str("take(source)}");
    expected.push_str("kept\n");
    let run = emit_link_and_run("arm-tail-break-independent-prefix", &text, "entry");
    super::boxed_enum_tests::assert_success(&run, expected.as_bytes());
}

#[test]
fn native_loop_arm_tail_break_mixed_modes_with_shared_bool_arguments() {
    let mut text = String::from(
        r#"fun view(source:String):borrow String from source=source
fun left(item:String):Unit {println("L");println(item)}
fun right(item:String):Unit {println("R");println(item)}
fun tick(own value:Int):Unit {}
fun take(own source:String):Unit {println(source)}
"#,
    );
    let mut entry =
        String::from("fun entry(){val source=\"kept\".clone();val yes=true;val no=false;\n");
    let mut expected = String::new();
    for mask in 0..8 {
        let mode = |bit| if mask & bit != 0 { "own " } else { "" };
        let (prefix, guard, arm) = (mode(1), mode(2), mode(4));
        text.push_str(&format!(r#"fun run{mask}({guard}flag:Boolean,{prefix}before:Boolean,source:String,{arm}choose:Boolean):Unit {{
 if(before){{tick(1)}}else{{tick(2)}}
 while(flag){{if(choose){{borrow val a=view(source);left(a)}}else{{borrow val b=view(source);right(b);right(b)}};break}}
 println("after-loop");println(source)
}}
"#));
        entry.push_str(&format!("println(\"mode-{mask}\");\n"));
        expected.push_str(&format!("mode-{mask}\n"));
        for flag in [false, true] {
            for before in [false, true] {
                for choose in [false, true] {
                    let name = |value| if value { "yes" } else { "no" };
                    entry.push_str(&format!(
                        "run{mask}({},{},source,{});\n",
                        name(flag),
                        name(before),
                        name(choose)
                    ));
                    if flag {
                        expected.push_str(if choose {
                            "L\nkept\n"
                        } else {
                            "R\nkept\nR\nkept\n"
                        });
                    }
                    expected.push_str("after-loop\nkept\n");
                }
            }
        }
    }
    entry.push_str("take(source)}");
    expected.push_str("kept\n");
    text.push_str(&entry);
    let run = emit_link_and_run("arm-tail-break-mixed-control-aliases", &text, "entry");
    super::boxed_enum_tests::assert_success(&run, expected.as_bytes());
}

#[test]
fn native_counter_arm_conditional_break_runs_zero_one_two_rounds() {
    run_counter_break_paths(false);
}

#[test]
fn native_counter_arm_conditional_break_shares_stable_flag() {
    run_counter_break_paths(true);
}

fn run_counter_break_paths(shared: bool) {
    let mut text = String::from(
        r#"fun view(source:String):borrow String from source=source
fun left(item:String):Unit {println("L");println(item)}
fun right(item:String):Unit {println("R");println(item)}
fun take(own source:String):Unit {println(source)}
"#,
    );
    let mut entry =
        String::from("fun entry(){val source=\"kept\".clone();val yes=true;val no=false;\n");
    let mut expected = String::new();
    for mask in 0..if shared { 4 } else { 8 } {
        let mode = |bit| if mask & bit != 0 { "own " } else { "" };
        let (rounds, choose, stop) = (mode(1), mode(2), mode(4));
        let stop_parameter = if shared {
            String::new()
        } else {
            format!("{stop}stop:Boolean,")
        };
        let stop_condition = if shared { "choose" } else { "stop" };
        text.push_str(&format!(r#"fun run{mask}({rounds}rounds:Int,{stop_parameter}source:String,{choose}choose:Boolean):Unit {{
 var remaining=rounds
 while(remaining>0){{if(choose){{borrow val a=view(source);left(a)}}else{{borrow val b=view(source);right(b);right(b)}};if({stop_condition}){{break}};remaining=remaining-1}}
 println("after-loop");println(source)
}}
"#));
        entry.push_str(&format!("println(\"mode-{mask}\");\n"));
        expected.push_str(&format!("mode-{mask}\n"));
        for rounds in 0..=2 {
            for choose in [false, true] {
                for stop in [false, true] {
                    if shared && stop != choose {
                        continue;
                    }
                    let name = |value| if value { "yes" } else { "no" };
                    let stop_argument = if shared {
                        String::new()
                    } else {
                        format!("{},", name(stop))
                    };
                    entry.push_str(&format!(
                        "run{mask}({rounds},{stop_argument}source,{});\n",
                        name(choose)
                    ));
                    let iterations = if stop { rounds.min(1) } else { rounds };
                    for _ in 0..iterations {
                        expected.push_str(if choose {
                            "L\nkept\n"
                        } else {
                            "R\nkept\nR\nkept\n"
                        });
                    }
                    expected.push_str("after-loop\nkept\n");
                }
            }
        }
    }
    entry.push_str("take(source)}");
    expected.push_str("kept\n");
    text.push_str(&entry);
    let label = if shared {
        "counter-arm-conditional-break-shared-flag"
    } else {
        "counter-arm-conditional-break"
    };
    let run = emit_link_and_run(label, &text, "entry");
    super::boxed_enum_tests::assert_success(&run, expected.as_bytes());
}
