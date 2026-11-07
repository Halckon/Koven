//! 原 nullable when 写入、捕获与 trial 诊断消费者的双入口观察。
use super::{super::codes, checked_entries};

#[test]
fn nullable_when_mixed_alternatives_do_not_narrow_body() {
    // A null alternative keeps the body nullable even when another alternative is non-null.
    let (sources, _, entries) = checked_entries(
        "fun extract(x: Boolean?): Boolean = when (x) { null, true -> x; else -> false }",
    );
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0084"]);
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .expect("body span"),
            "x"
        );
        assert!(typed.non_null_uses().is_empty());
    }
}

#[test]
fn nullable_when_body_mutation_does_not_leak_to_unselected_entry() {
    // The null body's mutation cannot execute on the path selecting else.
    let text = "fun extract(input: Int?, replacement: Int?): Int { var x = input
        return (when (x) { null -> { x = replacement; if (true) { 0 } else { 0 } }; else -> x })
    }";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    }
}

#[test]
fn nullable_when_condition_mutation_invalidates_source_proof() {
    // The tested subject is a snapshot; a later assignment changes the source binding.
    let text = "fun extract(input: Boolean?, replacement: Boolean?): Boolean { var x = input
        return (when (x) { null -> false;
            if (true) { x = replacement; if (true) { true } else { false } } else { false } -> false;
            else -> x })
    }";
    let (sources, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0084"]);
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .expect("body span"),
            "x"
        );
    }
}

#[test]
fn nullable_when_does_not_smart_cast_repeated_field_read() {
    // Each field read is a fresh source evaluation, not the internal subject identity.
    let text = "class Holder(val item: Int?)\n
        fun extract(h: Holder): Int = when (h.item) { null -> 0; else -> h.item }";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0084"]);
        assert!(typed.non_null_uses().is_empty());
    }
}

#[test]
fn nullable_when_failed_overload_trials_do_not_publish_plans() {
    // Neither candidate can return a nullable null branch as its non-null result.
    let text = "fun choose(callback: (Int?) -> Int): Int = 1
        fun choose(callback: (Boolean?) -> Boolean): Boolean = true
        fun use(): Unit { val selected = choose { when (it) { null -> it; else -> it } } }";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert!(!typed.diagnostics().is_empty());
        assert!(typed.nullable_whens().is_empty());
    }
}

#[test]
fn nullable_when_captured_mutable_binding_is_not_a_stable_source() {
    let text = "fun extract(input: Int?): Int { var x = input
        val capture: () -> Int? = { x }
        return (when (x) { null -> 0; else -> x })
    }";
    let (sources, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0084"]);
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .expect("body span"),
            "x"
        );
    }
}

#[test]
fn nullable_when_inout_condition_call_invalidates_subject_binding() {
    let text = "fun mutate(inout input: Boolean?): Boolean = true
        fun extract(input: Boolean?): Boolean { var x = input
            return (when (x) { mutate(&x) -> false; null -> false; else -> x })
        }";
    let (sources, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0084"]);
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .expect("body span"),
            "x"
        );
    }
}
