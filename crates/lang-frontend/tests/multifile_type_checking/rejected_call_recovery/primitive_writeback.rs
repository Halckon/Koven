use super::*;

#[test]
fn ownership_primitive_writeback_tracks_pending_root_values() {
    let declarations = "fun keep(value: Int): Int = value";
    let pending = r#"listOf("${listOf<Unbound>()}")"#;
    let replace_pending = format!("replace(&items, {pending})");
    for (entry, rejected, expected, retained) in [
        (
            format!(
                "{{ var items = listOf<String>(); replace(&items, {pending}); return keep(items.size) }}"
            ),
            vec!["keep(items.size)", "items.size", &replace_pending],
            vec![("L0165", "Unbound"), ("L0165", "items")],
            None,
        ),
        (
            format!(
                "{{ var items = {pending}; val old = replace(&items, listOf<String>()); keep(old.size); return keep(items.size) }}"
            ),
            vec![
                "keep(old.size)",
                "old.size",
                "replace(&items, listOf<String>())",
            ],
            vec![("L0165", "Unbound")],
            Some("keep(items.size)"),
        ),
        (
            format!(
                "{{ var items = listOf<String>(); val old = replace(&items, {pending}); keep(items.size); return keep(old.size) }}"
            ),
            vec!["keep(items.size)", "items.size", &replace_pending],
            vec![("L0165", "Unbound"), ("L0165", "items")],
            Some("keep(old.size)"),
        ),
        (
            format!(
                "{{ var left = {pending}; var right = listOf<String>(); swap(&left, &right); keep(right.size); return keep(left.size) }}"
            ),
            vec!["keep(right.size)", "right.size", "swap(&left, &right)"],
            vec![("L0165", "Unbound"), ("L0165", "right")],
            Some("keep(left.size)"),
        ),
        (
            format!(
                "{{ var left = listOf<String>(); var right = {pending}; swap(&left, &right); keep(left.size); return keep(right.size) }}"
            ),
            vec!["keep(left.size)", "left.size", "swap(&left, &right)"],
            vec![("L0165", "Unbound"), ("L0165", "left")],
            Some("keep(right.size)"),
        ),
        (
            format!(
                "{{ var items = listOf<String>(); loop {{ {replace_pending}; break }}; return keep(items.size) }}"
            ),
            vec!["keep(items.size)", "items.size", &replace_pending],
            vec![("L0165", "Unbound"), ("L0165", "items")],
            None,
        ),
    ] {
        assert_call_recovery_with_raw(declarations, &entry, &rejected, &[], &expected, retained);
    }
    let rejected = format!("replace(items, {pending})");
    assert_call_recovery_with_raw(
        declarations,
        &format!("{{ var items = listOf<String>(); {rejected}; return keep(items.size) }}"),
        &[&rejected],
        &[("L0122", "items")],
        &[("L0122", "items"), ("L0165", "Unbound")],
        Some("keep(items.size)"),
    );
}

#[test]
fn nested_replace_prefix_that_returns_does_not_write_back() {
    assert_call_recovery_with_raw(
        "fun flag(): Boolean = true\nfun keep(value: Int): Int = value",
        r#"{ var items = listOf<String>();
             var other = listOf("${listOf<Unbound>()}");
             if (flag()) { replace(&items, replace(&other, return 0)) };
             return keep(items.size) }"#,
        &[
            "replace(&items, replace(&other, return 0))",
            "replace(&other, return 0)",
        ],
        &[],
        &[("L0165", "Unbound")],
        Some("keep(items.size)"),
    );
}

#[test]
fn returning_initializer_excludes_later_primitive_writeback() {
    assert_call_recovery_with_raw(
        "fun flag(): Boolean = true\nfun keep(value: Int): Int = value",
        r#"{ var items = listOf<String>();
             var other = listOf<String>();
             if (flag()) {
                 val old = replace(&other, return 0);
                 replace(&items, listOf("${listOf<Unbound>()}"))
             };
             return keep(items.size) }"#,
        &[r#"replace(&items, listOf("${listOf<Unbound>()}"))"#],
        &[],
        &[("L0165", "Unbound")],
        Some("keep(items.size)"),
    );
}

#[test]
fn returning_replace_does_not_complete_an_enclosing_assignment() {
    assert_call_recovery_with_raw(
        "fun flag(): Boolean = true\nfun keep(value: Int): Int = value",
        r#"{ var items = listOf<String>();
             var other = listOf("${listOf<Unbound>()}");
             if (flag()) { items = replace(&other, return 0) };
             return keep(items.size) }"#,
        &[
            "items = replace(&other, return 0)",
            "replace(&other, return 0)",
        ],
        &[],
        &[("L0165", "Unbound")],
        Some("keep(items.size)"),
    );
}

#[test]
fn replace_does_not_publish_when_the_new_value_contains_deferred_evaluation() {
    assert_call_recovery_with_raw(
        "fun keep(value: String): Int = 0",
        r#"{ var text = "old";
             replace(&text, "${outside()}");
             return keep(text) }"#,
        &[r#"replace(&text, "${outside()}")"#, "keep(text)"],
        &[],
        &[("L0165", "outside"), ("L0165", "text")],
        None,
    );
}

#[test]
fn returning_place_prefix_excludes_a_break_in_the_second_operand() {
    let dirty = r#"replace(&items, listOf("${listOf<Unbound>()}"))"#;
    assert_call_recovery_with_raw(
        "fun flag(): Boolean = true\nfun keep(value: Int): Int = value",
        &format!(
            "{{ var items = listOf<String>();
                val a = arrayOf<Int>(0);
                loop {{
                    if (flag()) {{
                        replace(&a[return 0], if (flag()) {{ {dirty}; break }} else 0)
                    }};
                    break
                }};
                return keep(items.size) }}"
        ),
        &[dirty],
        &[],
        &[("L0165", "Unbound")],
        Some("keep(items.size)"),
    );
}
