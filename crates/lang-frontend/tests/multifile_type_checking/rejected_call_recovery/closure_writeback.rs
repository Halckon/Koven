use super::*;

const HELPERS: &str = "fun flag(): Boolean = true\nfun keep(value: Int): Int = value";
const DIRTY: &str = r#"items = listOf("${listOf<Unbound>()}")"#;

#[test]
fn invoking_a_closure_updates_the_captured_root_through_an_immutable_alias() {
    for call in ["update()", "(alias)()"] {
        assert_call_recovery_with_raw(
            HELPERS,
            &format!(
                "{{ var items = listOf<String>();
                         val update: () -> Unit = {{ if (flag()) {{ {DIRTY}; return }}; val end = 0 }};
                         val alias = update;
                         {call};
                         keep(items.size);
                         return keep(0) }}"
            ),
            &["items.size", "keep(items.size)"],
            &[],
            &[("L0165", "Unbound"), ("L0165", "items")],
            Some("keep(0)"),
        );
    }
}

#[test]
fn closure_formation_and_abrupt_paths_do_not_write_back() {
    for (body, call) in [
        (DIRTY.to_owned(), ""),
        (format!("if (true) {{ return }}; {DIRTY}"), "update()"),
        (
            format!("if (flag()) {{ {DIRTY}; stop() }}; val end = 0"),
            "update()",
        ),
    ] {
        assert_call_recovery_with_raw(
            &format!("{HELPERS}\nfun stop(): Nothing = stop()"),
            &format!(
                "{{ var items = listOf<String>();
                         val update: () -> Unit = {{ {body} }};
                         {call}
                         return keep(items.size) }}"
            ),
            &["listOf<Unbound>()"],
            &[],
            &[("L0165", "Unbound")],
            Some("keep(items.size)"),
        );
    }
}

#[test]
fn invoking_a_captured_write_is_visible_at_the_loop_head() {
    assert_call_recovery_with_raw(
        HELPERS,
        &format!(
            "{{ var items = listOf<String>();
                     val update: () -> Unit = {{ {DIRTY} }};
                     while (flag()) {{ keep(items.size); update(); continue }};
                     return keep(0) }}"
        ),
        &["items.size", "keep(items.size)"],
        &[],
        &[("L0165", "Unbound"), ("L0165", "items")],
        Some("keep(0)"),
    );
}

#[test]
fn literal_and_nested_calls_preserve_the_capture_origin() {
    for calls in [
        format!("({{ {DIRTY} }})();"),
        format!(
            "val update: () -> Unit = {{ {DIRTY} }}; val relay: () -> Unit = {{ update() }}; relay();"
        ),
    ] {
        assert_call_recovery_with_raw(
            HELPERS,
            &format!("{{ var items = listOf<String>(); {calls}; return keep(items.size) }}"),
            &["items.size", "keep(items.size)"],
            &[],
            &[("L0165", "Unbound"), ("L0165", "items")],
            None,
        );
    }
}

#[test]
fn passing_or_shadowing_a_closure_preserves_the_original_capture_state() {
    for action in [
        "ignore(update);",
        "if (true) { val update: () -> Unit = { val harmless = 0 }; update() };",
    ] {
        assert_call_recovery_with_raw(
            &format!("{HELPERS}\nfun ignore(action: () -> Unit): Unit {{}}"),
            &format!(
                "{{ var items = listOf<String>(); val update: () -> Unit = {{ {DIRTY} }};
                         {action} return keep(items.size) }}"
            ),
            &["listOf<Unbound>()"],
            &[],
            &[("L0165", "Unbound")],
            Some("keep(items.size)"),
        );
    }
}

#[test]
fn a_rejected_closure_call_leaves_the_capture_unchanged() {
    assert_call_recovery_with_raw(
        HELPERS,
        &format!(
            "{{ var items = listOf<String>(); val update: () -> Unit = {{ {DIRTY} }};
                     update(1); return keep(items.size) }}"
        ),
        &["update(1)"],
        &[("L0121", "1")],
        &[("L0165", "Unbound"), ("L0121", "1")],
        Some("keep(items.size)"),
    );
}

#[test]
fn a_known_diverging_closure_does_not_create_a_normal_writeback_edge() {
    assert_call_recovery_with_raw(
        &format!("{HELPERS}\nfun stop(): Nothing = stop()"),
        &format!(
            "{{ var items = listOf<String>();
                     val halt: () -> Unit = {{ stop() }};
                     val update: () -> Unit = {{ halt(); {DIRTY} }};
                     update(); return keep(items.size) }}"
        ),
        &["listOf<Unbound>()"],
        &[],
        &[("L0165", "Unbound")],
        Some("keep(items.size)"),
    );
}

#[test]
fn a_closure_argument_that_returns_prevents_capture_writeback() {
    assert_call_recovery_with_raw(
        HELPERS,
        &format!(
            "{{ var items = listOf<String>();
                     val update: (Int) -> Unit = {{ ignored -> {DIRTY} }};
                     if (flag()) {{ update(return 0); keep(items.size) }};
                     return keep(0) }}"
        ),
        &["listOf<Unbound>()"],
        &[],
        &[("L0165", "Unbound")],
        Some("keep(items.size)"),
    );
}

#[test]
fn an_invalid_closure_return_does_not_prove_that_the_call_diverges() {
    assert_call_recovery_with_raw(
        HELPERS,
        &format!(
            "{{ var items = listOf<String>();
                     val bad: () -> Unit = {{ return 0; val tail = 0 }};
                     val relay: () -> Unit = {{ bad(); {DIRTY} }};
                     relay(); return keep(items.size) }}"
        ),
        &["items.size", "keep(items.size)"],
        &[("L0087", "0")],
        &[("L0087", "0"), ("L0165", "Unbound"), ("L0165", "items")],
        None,
    );
}
