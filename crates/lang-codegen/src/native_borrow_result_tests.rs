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
