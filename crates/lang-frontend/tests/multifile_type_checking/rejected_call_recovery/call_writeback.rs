use super::*;

const DIRTY: &str = r#"items = listOf("${listOf<Unbound>()}")"#;
const CALLER: &str =
    "fun caller(): Int { var items = listOf<String>(); update(&items); return keep(items.size) }";
const HELPERS: &str = "fun flag(): Boolean = true\nfun keep(value: Int): Int = value";

#[test]
fn named_inout_pending_return_is_independent_of_declaration_order() {
    let update = format!(
        "fun update(inout items: List<String>): Unit {{ if (flag()) {{ {DIRTY}; return }} }}"
    );
    for (declarations, diagnostics) in [
        (
            format!("{HELPERS}\n{CALLER}\n{update}"),
            [("L0165", "items"), ("L0165", "Unbound")],
        ),
        (
            format!("{HELPERS}\n{update}\n{CALLER}"),
            [("L0165", "Unbound"), ("L0165", "items")],
        ),
    ] {
        assert_call_recovery_with_raw(
            &declarations,
            "= 0",
            &["items.size", "keep(items.size)"],
            &[],
            &diagnostics,
            Some("update(&items)"),
        );
    }
}

#[test]
fn named_inout_writeback_reaches_a_transitive_caller() {
    assert_call_recovery_with_raw(
        &format!(
            "{HELPERS}
             {CALLER}
             fun update(inout items: List<String>): Unit {{ inner(&items) }}
             fun inner(inout items: List<String>): Unit {{ {DIRTY} }}"
        ),
        "= 0",
        &["items.size", "keep(items.size)"],
        &[],
        &[("L0165", "items"), ("L0165", "Unbound")],
        Some("inner(&items)"),
    );
}

#[test]
fn only_normal_returns_carry_a_pending_inout_write() {
    for (body, diagnostics) in [
        (
            format!("{DIRTY}; items = listOf<String>()"),
            &[("L0165", "Unbound"), ("L0165", "items")][..],
        ),
        (
            format!("if (true) {{ return }}; {DIRTY}"),
            &[("L0165", "Unbound")][..],
        ),
        (
            format!("if (flag()) {{ {DIRTY}; stop() }}"),
            &[("L0165", "Unbound")][..],
        ),
        (
            format!(
                "when (when (flag()) {{ else -> if (true) {{ return }} else 0 }}) {{
                    else -> {{ {DIRTY}; return }}
                }}"
            ),
            &[("L0165", "Unbound")][..],
        ),
        (
            format!("when (return) {{ else -> {{ {DIRTY}; return }} }}"),
            &[("L0165", "Unbound")][..],
        ),
        (
            format!("val unused: () -> Unit = {{ {DIRTY}; return }}"),
            &[("L0165", "Unbound")][..],
        ),
        (
            format!("if (flag()) {{ consume(return); {DIRTY} }}"),
            &[("L0165", "Unbound")][..],
        ),
        (
            format!("sink(return, if (flag()) {{ {DIRTY}; return }} else 0)"),
            &[("L0165", "Unbound")][..],
        ),
        (
            format!("genericSink(return, if (flag()) {{ {DIRTY}; return }} else 0)"),
            &[("L0165", "Unbound")][..],
        ),
    ] {
        assert_call_recovery_with_raw(
            &format!(
                "{HELPERS}
                 fun stop(): Nothing = stop()
                 fun consume(value: Int): Unit {{}}
                 fun sink(first: Int, second: Int): Unit {{}}
                 fun <T> genericSink(first: Int, second: T): Unit {{}}
                 fun update(inout items: List<String>): Unit {{ {body} }}
                 {CALLER}"
            ),
            "= 0",
            &["listOf<Unbound>()"],
            &[],
            diagnostics,
            Some("keep(items.size)"),
        );
    }
}

#[test]
fn exiting_receiver_does_not_write_back_to_an_inout_argument() {
    assert_call_recovery_with_raw(
        &format!(
            "{HELPERS}
             class Mutator {{ fun update(inout items: List<String>): Unit {{ {DIRTY} }} }}
             fun relay(mutator: Mutator, inout items: List<String>): Unit {{
                 (if (true) {{ return }} else {{ mutator }}).update(&items)
             }}"
        ),
        "{ var items = listOf<String>(); relay(Mutator(), &items); return keep(items.size) }",
        &["listOf<Unbound>()"],
        &[],
        &[("L0165", "Unbound")],
        Some("keep(items.size)"),
    );
}

#[test]
fn writeback_survives_a_selected_call_with_a_pending_inout_operand() {
    for (first, second, parameter) in [
        (
            DIRTY,
            "second(&scratch, &items)",
            "inout scratch: List<String>",
        ),
        (
            "inner(&items)",
            "second(&scratch, &items)",
            "inout scratch: List<String>",
        ),
        (DIRTY, "second(keep(scratch.size), &items)", "count: Int"),
        (
            "inner(&items)",
            "second(keep(scratch.size), &items)",
            "count: Int",
        ),
    ] {
        let inner = if first == DIRTY {
            String::new()
        } else {
            format!("fun inner(inout items: List<String>): Unit {{ {DIRTY} }}")
        };
        assert_call_recovery_with_raw(
            &format!(
                "{HELPERS}
                 {CALLER}
                 fun update(inout items: List<String>): Unit {{
                     var scratch = listOf<String>();
                     first(&scratch);
                     {second}
                 }}
                 fun first(inout items: List<String>): Unit {{ {first} }}
                 {inner}
                 fun second({parameter}, inout items: List<String>): Unit {{ {DIRTY} }}"
            ),
            "= 0",
            &["keep(items.size)", "items.size", second],
            &[],
            &[
                ("L0165", "items"),
                ("L0165", "scratch"),
                ("L0165", "Unbound"),
                ("L0165", "Unbound"),
            ],
            Some("update(&items)"),
        );
    }
}

#[test]
fn named_writeback_uses_the_selected_parameter_mapping() {
    let call = "update(items = &items, other = &other)";
    assert_call_recovery_with_raw(
        &format!("{HELPERS}
                  fun update(inout other: List<String>, inout items: List<String>): Unit {{ {DIRTY} }}"),
        &format!("{{ var items = listOf<String>(); var other = listOf<String>();
                     {call}; keep(items.size); return keep(other.size) }}"),
        &["items.size", "keep(items.size)"],
        &[],
        &[("L0165", "Unbound"), ("L0165", "items")],
        Some("keep(other.size)"),
    );
}

#[test]
fn rejected_call_with_an_exiting_operand_does_not_fabricate_writeback() {
    let rejected = "consume(return, 0)";
    assert_call_recovery_with_raw(
        &format!(
            "{HELPERS}
                  fun consume(value: Int): Unit {{}}
                  fun update(inout items: List<String>): Unit {{ {rejected}; {DIRTY} }}
                  {CALLER}"
        ),
        "= 0",
        &[rejected],
        &[("L0121", "0")],
        &[("L0121", "0"), ("L0165", "Unbound")],
        Some("keep(items.size)"),
    );
}

#[test]
fn exiting_non_callable_callee_does_not_collect_argument_returns() {
    let rejected =
        format!("(if (true) {{ return }} else 0)(if (flag()) {{ {DIRTY}; return }} else 0)");
    assert_call_recovery_with_raw(
        &format!(
            "{HELPERS}
                  fun update(inout items: List<String>): Unit {{ {rejected} }}
                  {CALLER}"
        ),
        "= 0",
        &[&rejected],
        &[("L0119", "(if (true) { return } else 0)")],
        &[
            ("L0119", "(if (true) { return } else 0)"),
            ("L0165", "Unbound"),
        ],
        Some("keep(items.size)"),
    );
}

#[test]
fn known_nothing_call_with_a_pending_operand_still_has_no_normal_exit() {
    assert_call_recovery_with_raw(
        &format!(
            "{HELPERS}
                  fun stop(value: List<String>): Nothing = stop(value)
                  fun update(inout items: List<String>): Unit {{
                      var scratch = listOf<String>();
                      first(&scratch);
                      stop(scratch);
                      {DIRTY}
                  }}
                  {CALLER}
                  fun first(inout items: List<String>): Unit {{ {DIRTY} }}"
        ),
        "= 0",
        &["stop(scratch)"],
        &[],
        &[
            ("L0165", "scratch"),
            ("L0165", "Unbound"),
            ("L0165", "Unbound"),
        ],
        Some("keep(items.size)"),
    );
}

#[test]
fn an_unselected_overload_does_not_contribute_writeback() {
    assert_call_recovery_with_raw(
        &format!(
            "{HELPERS}
             fun update(inout items: List<String>, tag: Int): Unit {{ {DIRTY} }}
             fun update(inout items: List<String>, tag: Boolean): Unit {{}}"
        ),
        "{ var items = listOf<String>(); update(&items, false); return keep(items.size) }",
        &["listOf<Unbound>()"],
        &[],
        &[("L0165", "Unbound")],
        Some("keep(items.size)"),
    );
}

#[test]
fn a_named_writeback_is_replayed_across_continue() {
    assert_call_recovery_with_raw(
        &format!("{HELPERS}\nfun update(inout items: List<String>): Unit {{ {DIRTY} }}"),
        r#"{ var items = listOf<String>();
             while (flag()) {
                 keep(items.size);
                 update(&items);
                 continue
             };
             return keep(0) }"#,
        &["keep(items.size)", "items.size", "update(&items)"],
        &[],
        &[("L0165", "Unbound"), ("L0165", "items"), ("L0165", "items")],
        Some("keep(0)"),
    );
}

#[test]
fn deferred_index_keeps_a_known_read_only_inout_operand_invalid() {
    let rejected = "update(&items, &readOnly[keep(scratch.size)])";
    assert_call_recovery_with_raw(
        &format!(
            "{HELPERS}
                  fun first(inout items: List<String>): Unit {{ {DIRTY} }}
                  fun update(inout items: List<String>, inout count: Int): Unit {{ {DIRTY} }}"
        ),
        &format!(
            "{{ var scratch = listOf<String>(); first(&scratch);
                     val readOnly = listOf<Int>(0); var items = listOf<String>();
                     {rejected}; return keep(items.size) }}"
        ),
        &[rejected],
        &[("L0122", "&")],
        &[
            ("L0165", "Unbound"),
            ("L0165", "Unbound"),
            ("L0122", "&"),
            ("L0165", "scratch"),
        ],
        Some("keep(items.size)"),
    );
}

#[test]
fn only_reachable_when_selector_edges_contribute_pending_writes() {
    for (selector, diagnostics) in [
        ("flag() && pred(return)", &[("L0165", "Unbound")][..]),
        (
            "flag() || pred(return)",
            &[("L0165", "Unbound"), ("L0165", "items")][..],
        ),
        ("!(flag() || pred(return))", &[("L0165", "Unbound")][..]),
        (
            "(when (flag()) { else -> flag() && pred(return) })",
            &[("L0165", "Unbound")][..],
        ),
        (
            "(when (flag()) { else -> flag() || pred(return) })",
            &[("L0165", "Unbound"), ("L0165", "items")][..],
        ),
    ] {
        let (rejected, retained) = if diagnostics.len() == 2 {
            (["listOf<Unbound>()", "keep(items.size)"].as_slice(), None)
        } else {
            (["listOf<Unbound>()"].as_slice(), Some("keep(items.size)"))
        };
        assert_call_recovery_with_raw(
            &format!(
                "{HELPERS}
                 fun pred(value: Int): Boolean = value == 1
                 fun update(inout items: List<String>): Unit {{
                     when {{ {selector} -> {{ {DIRTY}; return }}; else -> return }}
                 }}
                 {CALLER}"
            ),
            "= 0",
            rejected,
            &[],
            diagnostics,
            retained,
        );
    }
}
