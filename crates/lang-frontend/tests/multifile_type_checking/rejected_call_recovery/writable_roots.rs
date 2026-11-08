use super::*;

#[test]
fn inout_parameter_root_tracks_writes_inside_its_callable() {
    for write in [
        r#"items = listOf("${listOf<Unbound>()}")"#,
        r#"replace(&items, listOf("${listOf<Unbound>()}"))"#,
    ] {
        assert_call_recovery_with_raw(
            &format!(
                "fun keep(value: Int): Int = value
                 fun update(inout items: List<String>): Int {{
                     {write};
                     return keep(items.size)
                 }}"
            ),
            "= 0",
            &[write, "items.size", "keep(items.size)"],
            &[],
            &[("L0165", "Unbound"), ("L0165", "items")],
            None,
        );
    }
}

#[test]
fn captured_var_remains_a_writable_root_in_the_enclosing_callable() {
    for write in [
        r#"items = listOf("${listOf<Unbound>()}")"#,
        r#"replace(&items, listOf("${listOf<Unbound>()}"))"#,
    ] {
        assert_call_recovery_with_raw(
            "fun keep(value: Int): Int = value",
            &format!(
                "{{ var items = listOf<String>();
                    val read: () -> List<String> = {{ items }};
                    {write};
                    return keep(items.size) }}"
            ),
            &[write, "items.size", "keep(items.size)"],
            &[],
            &[("L0165", "Unbound"), ("L0165", "items")],
            None,
        );
    }
}

#[test]
fn capture_body_writes_do_not_run_when_the_closure_is_formed() {
    let write = r#"items = listOf("${listOf<Unbound>()}")"#;
    assert_call_recovery_with_raw(
        "fun keep(value: Int): Int = value",
        &format!(
            "{{ var items = listOf<String>();
                val update: () -> Int = {{ {write}; keep((items).size) }};
                return keep(items.size) }}"
        ),
        &[write, "(items).size", "keep((items).size)"],
        &[],
        &[("L0165", "Unbound"), ("L0165", "items")],
        Some("keep(items.size)"),
    );
}

#[test]
fn inout_roots_follow_member_and_contextual_lambda_parameters() {
    let write = r#"items = listOf("${listOf<Unbound>()}")"#;
    for (declarations, entry) in [
        (
            format!(
                "fun keep(value: Int): Int = value
                 class Writer {{
                     fun update(inout items: List<String>): Int {{ {write}; return keep(items.size) }}
                 }}"
            ),
            "= 0".to_owned(),
        ),
        (
            "fun keep(value: Int): Int = value".to_owned(),
            format!(
                "{{ val update: (inout List<String>) -> Int = {{ items -> {write}; keep(items.size) }};
                    return 0 }}"
            ),
        ),
    ] {
        assert_call_recovery_with_raw(
            &declarations,
            &entry,
            &[write, "items.size", "keep(items.size)"],
            &[],
            &[("L0165", "Unbound"), ("L0165", "items")],
            None,
        );
    }
}

#[test]
fn captured_root_is_included_in_the_loop_backedge() {
    assert_call_recovery_with_raw(
        "fun flag(): Boolean = true\nfun keep(value: Int): Int = value",
        r#"{ var items = listOf<String>();
             val read: () -> List<String> = { items };
             while (flag()) {
                 keep(items.size);
                 items = listOf("${listOf<Unbound>()}");
                 continue
             };
             return keep(0) }"#,
        &["items.size", "keep(items.size)"],
        &[],
        &[("L0165", "items"), ("L0165", "items"), ("L0165", "Unbound")],
        Some("keep(0)"),
    );
}

#[test]
fn inout_exchange_transfers_pending_state_to_the_other_parameter() {
    assert_call_recovery_with_raw(
        r#"fun keep(value: Int): Int = value
           fun exchange(inout a: List<String>, inout b: List<String>): Int {
               a = listOf("${listOf<Unbound>()}");
               swap(&a, &b);
               keep(b.size);
               return keep(a.size)
           }"#,
        "= 0",
        &["swap(&a, &b)", "b.size", "keep(b.size)"],
        &[],
        &[("L0165", "Unbound"), ("L0165", "a"), ("L0165", "b")],
        Some("keep(a.size)"),
    );
}

#[test]
fn unresolved_operand_results_still_block_dependent_inout_reads() {
    assert_call_recovery_with_raw(
        r#"fun <T> pending(): List<String> = listOf<String>()
           fun keep(value: Int): Int = value
           fun update(inout items: List<String>): Int {
               replace(&items, pending<Unbound>());
               return keep(items.size)
           }"#,
        "= 0",
        &[
            "replace(&items, pending<Unbound>())",
            "items.size",
            "keep(items.size)",
        ],
        &[],
        &[("L0165", "Unbound"), ("L0165", "items")],
        None,
    );
    assert_call_recovery_with_raw(
        r#"fun keep(value: Int): Int = value
           fun exchange(inout a: Unbound, inout b: List<String>): Int {
               swap(&a, &b);
               return keep(b.size)
           }"#,
        "= 0",
        &["swap(&a, &b)", "b.size", "keep(b.size)"],
        &[],
        &[("L0165", "Unbound"), ("L0165", "a"), ("L0165", "b")],
        None,
    );
    assert_call_recovery_with_raw(
        r#"fun <T> pending(): List<String> = listOf<String>()
           fun keep(value: Int): Int = value
           fun update(inout items: List<String>): Int {
               replace(items, pending<Unbound>());
               return keep(items.size)
           }"#,
        "= 0",
        &["replace(items, pending<Unbound>())"],
        &[("L0122", "items")],
        &[("L0122", "items"), ("L0165", "Unbound")],
        Some("keep(items.size)"),
    );
}

#[test]
fn deferred_inout_replace_is_visible_across_a_continue() {
    assert_call_recovery_with_raw(
        r#"fun <T> pending(): List<String> = listOf<String>()
           fun flag(): Boolean = true
           fun keep(value: Int): Int = value
           fun update(inout items: List<String>): Int {
               while (flag()) {
                   keep(items.size);
                   replace(&items, pending<Unbound>());
                   continue
               };
               return keep(0)
           }"#,
        "= 0",
        &[
            "replace(&items, pending<Unbound>())",
            "items.size",
            "keep(items.size)",
        ],
        &[],
        &[("L0165", "items"), ("L0165", "items"), ("L0165", "Unbound")],
        Some("keep(0)"),
    );
}

#[test]
fn unresolved_primitive_type_argument_does_not_hide_a_known_writeback_conflict() {
    for (entry, primitive, retained, diagnostics) in [
        (
            r#"{ var items = listOf<String>();
                 replace<Unbound>(&items, "bad");
                 return keep(items.size) }"#,
            r#"replace<Unbound>(&items, "bad")"#,
            "keep(items.size)",
            &[("L0165", "Unbound")][..],
        ),
        (
            r#"{ var items = listOf("${listOf<Unbound>()}");
                 var text = "clean";
                 swap<Other>(&items, &text);
                 return keepText(text) }"#,
            "swap<Other>(&items, &text)",
            "keepText(text)",
            &[("L0165", "Unbound"), ("L0165", "Other")][..],
        ),
        (
            r#"{ var items = listOf("${listOf<Unbound>()}");
                 replace<Other>(&items, listOf<String>());
                 return keep(items.size) }"#,
            "replace<Other>(&items, listOf<String>())",
            "keep(items.size)",
            &[("L0165", "Unbound"), ("L0165", "Other")][..],
        ),
    ] {
        assert_call_recovery_with_raw(
            "fun keep(value: Int): Int = value\nfun keepText(value: String): Int = 0",
            entry,
            &[primitive],
            &[],
            diagnostics,
            Some(retained),
        );
    }
}
