//! Explicit external TypeRef identity and candidate-only evidence.

use super::*;

#[test]
fn pending_storage_type_references_keep_known_shape_and_rejections() {
    let declarations = "fun <T> take(): Int = 0";
    for ty in [
        "List<Unbound>",
        "List<Unbound?>",
        "Array<Unbound>",
        "Rc<Unbound>",
        "Map<Int, Unbound>",
        "Map<Unbound, Int>",
        "Box<Unbound>",
    ] {
        let primary = if ty.contains('?') {
            "Unbound?"
        } else {
            "Unbound"
        };
        assert_call_recovery(
            declarations,
            &format!("take<{ty}>()"),
            None,
            &[("L0165", primary)],
            None,
        );
    }
    for (ty, code, primary) in [
        ("List<Nothing>", "L0125", "Nothing"),
        ("Array<Nothing?>", "L0125", "Nothing?"),
        ("List<Any>", "L0125", "Any"),
        ("Box<Unbound?>", "L0117", "Unbound?"),
        ("Map<Float, Int>", "L0161", "Float"),
    ] {
        assert_call_recovery(
            declarations,
            &format!("take<{ty}>()"),
            Some((code, primary)),
            &[(code, primary)],
            None,
        );
    }
    assert_call_recovery(
        declarations,
        "take<Map<Float, Unbound>>()",
        Some(("L0161", "Float")),
        &[("L0161", "Float"), ("L0165", "Unbound")],
        None,
    );
}

#[test]
fn unbound_signature_storage_uses_source_diagnostics() {
    for ty in [
        "Unbound",
        "List<Unbound>",
        "Box<Unbound>",
        "Map<Int, Unbound>",
        "Map<Unbound, Int>",
    ] {
        assert_call_recovery(
            &format!("fun inspect(value: {ty}): Int = 0\nfun <T> take(): Int = 0"),
            "take<Other>()",
            None,
            &[("L0165", "Unbound"), ("L0165", "Other")],
            None,
        );
    }
}

#[test]
fn construction_with_pending_signature_uses_source_diagnostics() {
    for (field, argument, concrete) in [
        ("Unbound", "1", None),
        ("List<Unbound>", "listOf<Int>(1)", Some("listOf<Int>(1)")),
    ] {
        let call = format!("Holder({argument})");
        assert_call_recovery_with_raw(
            &format!("class Holder(val item: {field})"),
            &format!("{{ val item = {call}; return 0 }}"),
            &[&call],
            &[],
            &[("L0165", "Unbound"), ("L0165", &call)],
            concrete,
        );
    }
    let initializer = "1";
    let call = format!("Holder({initializer})");
    assert_call_recovery_with_raw(
        "class Holder(val item: List<Unbound>)",
        &format!("{{ val item = {call}; return 0 }}"),
        &[&call],
        &[("L0084", initializer)],
        &[("L0165", "Unbound"), ("L0084", initializer)],
        None,
    );
    let call = "Maybe.Some(1)";
    assert_call_recovery_with_raw(
        "enum class Maybe { Some(item: Unbound), None }",
        &format!("{{ val item = {call}; return 0 }}"),
        &[call],
        &[],
        &[("L0165", "Unbound"), ("L0165", call)],
        None,
    );
}

#[test]
fn pending_container_annotations_do_not_publish_construction() {
    for (ty, initializer) in [
        ("List<Unbound>", "listOf()"),
        ("Map<Int, Unbound>", "mapOf()"),
    ] {
        assert_call_recovery_in_entry(
            "",
            &format!("{{ val value: {ty} = {initializer}; return 0 }}"),
            initializer,
            None,
            &[("L0165", "Unbound")],
            None,
        );
    }
    for initializer in ["listOf<Unbound>()", "mapOf<Int, Unbound>()"] {
        assert_call_recovery_in_entry(
            "",
            &format!("{{ val value: Int = {initializer}; return 0 }}"),
            initializer,
            Some(("L0084", initializer)),
            &[("L0084", initializer), ("L0165", "Unbound")],
            None,
        );
    }
    assert_call_recovery_with_raw(
        "fun <T> take(): Int = 0",
        "{ val value: List<Unbound> = listOf<Int>(1); return take<Other>() }",
        &["take<Other>()"],
        &[],
        &[("L0165", "Unbound"), ("L0165", "Other")],
        Some("listOf<Int>(1)"),
    );
    assert_call_recovery_in_entry(
        "",
        "{ val value: Array<Unbound> = listOf<Int>(1); return 0 }",
        "listOf<Int>(1)",
        Some(("L0084", "listOf<Int>(1)")),
        &[("L0165", "Unbound"), ("L0084", "listOf<Int>(1)")],
        None,
    );
    assert_call_recovery_in_entry(
        "",
        "{ val value: List<Unbound> = listOf(outside()); return 0 }",
        "listOf(outside())",
        None,
        &[("L0165", "Unbound"), ("L0165", "outside")],
        None,
    );
    let invalid = "mapOf<Int, Unbound>(outside())";
    assert_call_recovery(
        "",
        invalid,
        Some(("L0121", invalid)),
        &[
            ("L0121", invalid),
            ("L0165", "Unbound"),
            ("L0165", "outside"),
        ],
        None,
    );
}

#[test]
fn pending_storage_does_not_publish_dependent_call_or_size() {
    let call = r#"keep("${listOf<Unbound>()}")"#;
    assert_call_recovery_with_raw(
        "fun keep(value: String): Int = 0",
        &format!("= {call}"),
        &[call, "listOf<Unbound>()"],
        &[],
        &[("L0165", "Unbound")],
        None,
    );
    for factory in ["listOf<Unbound>()", "mapOf<Int, Unbound>()"] {
        let size = format!("{factory}.size");
        let call = format!("keep({size})");
        assert_call_recovery_with_raw(
            "fun keep(value: Int): Int = 0",
            &format!("= {call}"),
            &[&call, &size, factory],
            &[],
            &[("L0165", "Unbound")],
            None,
        );
        assert_call_recovery_in_entry(
            "",
            &format!("{{ val value: Boolean = {size}; return 0 }}"),
            &size,
            Some(("L0084", &size)),
            &[("L0084", &size), ("L0165", "Unbound")],
            None,
        );
    }
    for outer in [
        r#"listOf("${listOf<Unbound>()}")"#,
        r#"Holder("${listOf<Unbound>()}")"#,
        r#"Rc("${listOf<Unbound>()}")"#,
        r#""${listOf<Unbound>()}".clone()"#,
    ] {
        let call = format!("keep({outer})");
        assert_call_recovery_with_raw(
            "class Holder(val text: String)\nfun <T> keep(value: T): Int = 0",
            &format!("= {call}"),
            &[&call, outer, "listOf<Unbound>()"],
            &[],
            &[("L0165", "Unbound")],
            None,
        );
    }
    for receiver in [
        r#"if (true) listOf("${listOf<Unbound>()}") else listOf<String>()"#,
        r#"if (true) { val pending = listOf<Unbound>(); mapOf<Int, String>() } else mapOf<Int, String>()"#,
    ] {
        let size = format!("({receiver}).size");
        let call = format!("keep({size})");
        assert_call_recovery_with_raw(
            "fun keep(value: Int): Int = 0",
            &format!("= {call}"),
            &[&call, &size, "listOf<Unbound>()"],
            &[],
            &[("L0165", "Unbound")],
            None,
        );
    }
}

#[test]
fn pending_storage_crosses_immutable_local_aliases_without_leaking_to_shadowed_symbols() {
    assert_call_recovery_with_raw(
        "fun keep(value: Int): Int = value",
        r#"{ val items = listOf("${listOf<Unbound>()}");
             val alias = items
             if (true) { val items = listOf<String>(); keep(items.size) }
             return keep(alias.size) }"#,
        &["keep(alias.size)", "alias.size", "listOf<Unbound>()"],
        &[],
        &[("L0165", "Unbound")],
        Some("keep(items.size)"),
    );
    assert_call_recovery_with_raw(
        "fun keep(value: String): Int = 0",
        r#"{ val text: String = "${listOf<Unbound>()}";
             keep("clean")
             return keep(text) }"#,
        &["keep(text)", "listOf<Unbound>()"],
        &[],
        &[("L0165", "Unbound")],
        Some(r#"keep("clean")"#),
    );
    assert_call_recovery_with_raw(
        "fun keep(value: Int): Int = value",
        r#"{ var items = listOf("${listOf<Unbound>()}");
             items = listOf<String>()
             return keep(items.size) }"#,
        &["listOf<Unbound>()"],
        &[],
        &[("L0165", "Unbound")],
        Some("keep(items.size)"),
    );
}

#[test]
fn pending_local_storage_includes_annotations_and_rolled_back_lambda_calls() {
    assert_call_recovery_with_raw(
        "fun keep(value: String): Int = 0",
        r#"{ val items: List<Unbound> = listOf<Int>(1);
             val text = "${items}"
             return keep(text) }"#,
        &["keep(text)"],
        &[],
        &[("L0165", "Unbound")],
        Some("listOf<Int>(1)"),
    );
    let choose = "choose({ ignored -> val captured = text; 1 })";
    assert_call_recovery_with_raw(
        "fun choose(cb: (borrow Int) -> Int): Int = 0\n\
         fun choose(cb: (borrow String) -> String): Int = 1\n\
         fun keep(value: Int): Int = value",
        &format!(
            r#"{{ val text = "${{listOf<Unbound>()}}";
                 val result: Int = {choose}
                 return keep(result) }}"#
        ),
        &["keep(result)", choose, "listOf<Unbound>()"],
        &[],
        &[("L0165", "Unbound"), ("L0165", choose)],
        None,
    );
}

#[test]
fn pending_index_and_integer_operands_do_not_publish_descriptors() {
    for (declaration, locals, operation, retained) in [
        (
            "fun keep(value: String): Int = 0",
            "val alias = items; listOf<Int>(1)[0]",
            "alias[0]",
            "listOf<Int>(1)[0]",
        ),
        (
            "fun keep(value: Int?): Int = 0",
            "val key = items.size; val map = mapOf<Int, Int>(); map[0]",
            "map[key]",
            "map[0]",
        ),
        (
            "fun keep(value: Int?): Int = 0",
            "val key = items.size; val map = mapOf<Int, Int>(); map.get(0)",
            "map.get(key)",
            "map.get(0)",
        ),
        (
            "fun keep(value: Int): Int = value",
            "val count = items.size; 1.inv()",
            "count.inv()",
            "1.inv()",
        ),
    ] {
        let call = format!("keep({operation})");
        assert_call_recovery_with_raw(
            declaration,
            &format!(
                r#"{{ val items = listOf("${{listOf<Unbound>()}}"); {locals}; return {call} }}"#
            ),
            &[&call, operation],
            &[],
            &[("L0165", "Unbound")],
            Some(retained),
        );
    }
}

#[test]
fn pending_indices_and_integer_operations_preserve_known_errors() {
    for (statement, operation, code, primary) in [
        ("items[true]", "items[true]", "L0128", "true"),
        (r#"items[0] = "text""#, "items[0]", "L0129", "="),
        ("items.size.inv(1)", "items.size.inv(1)", "L0121", "1"),
        (
            "items.size.inv<Int>()",
            "items.size.inv<Int>()",
            "L0091",
            "inv",
        ),
        (
            "val wrong: Boolean = items.size.inv()",
            "items.size.inv()",
            "L0084",
            "items.size.inv()",
        ),
    ] {
        assert_call_recovery_with_raw(
            "",
            &format!(
                r#"{{ val items = listOf("${{listOf<Unbound>()}}"); {statement}; return 0 }}"#
            ),
            &[operation],
            &[(code, primary)],
            &[("L0165", "Unbound"), (code, primary)],
            None,
        );
    }
    let target = r#"(if (true) listOf("${listOf<Unbound>()}") else listOf<String>())[0]"#;
    assert_call_recovery_with_raw(
        "",
        &format!(r#"{{ {target} = "text"; return 0 }}"#),
        &[target],
        &[("L0129", "=")],
        &[("L0165", "Unbound"), ("L0129", "=")],
        None,
    );
}

#[test]
fn pending_map_writes_preserve_write_role_and_independent_operands() {
    for (write, target, raw, diagnostics) in [
        ("map[1] = count", "map[1]", None, vec![("L0165", "Unbound")]),
        (
            "map.put(1, count)",
            "map.put(1, count)",
            None,
            vec![("L0165", "Unbound")],
        ),
        (
            "map[count] = outside()",
            "map[count]",
            None,
            vec![("L0165", "Unbound"), ("L0165", "outside")],
        ),
        (
            "map[count] = true",
            "map[count]",
            Some(("L0084", "true")),
            vec![("L0165", "Unbound"), ("L0084", "true")],
        ),
        (
            "map[count] += 1",
            "map[count]",
            Some(("L0130", "+=")),
            vec![("L0165", "Unbound"), ("L0130", "+=")],
        ),
        (
            "named[count] = 1",
            "named[count]",
            Some(("L0084", "count")),
            vec![("L0165", "Unbound"), ("L0084", "count")],
        ),
        (
            "named.put(count, 1)",
            "named.put(count, 1)",
            Some(("L0084", "count")),
            vec![("L0165", "Unbound"), ("L0084", "count")],
        ),
        (
            "readonly[1] = count",
            "readonly[1]",
            Some(("L0129", "=")),
            vec![("L0165", "Unbound"), ("L0129", "=")],
        ),
    ] {
        assert_call_recovery_with_raw(
            "",
            &format!(
                r#"{{ val items = listOf("${{listOf<Unbound>()}}");
                     val count = items.size
                     val map = mutableMapOf<Int, Int>()
                     val named = mutableMapOf<String, Int>()
                     val readonly = mapOf<Int, Int>()
                     map[0] = 1
                     {write}
                     return 0 }}"#
            ),
            &[target],
            raw.as_slice(),
            &diagnostics,
            Some("map[0]"),
        );
    }
}

#[test]
fn pending_element_places_do_not_publish_ownership_primitives() {
    for (operation, raw, final_diagnostics) in [
        (
            r#"replace(&items[0], "new")"#,
            vec![],
            vec![("L0165", "Unbound")],
        ),
        (
            "swap(&items[0], &other[0])",
            vec![],
            vec![("L0165", "Unbound")],
        ),
        (
            "replace(&items[0], 1)",
            vec![("L0084", "1")],
            vec![("L0165", "Unbound"), ("L0084", "1")],
        ),
    ] {
        assert_call_recovery_with_raw(
            "",
            &format!(
                r#"{{ val items = arrayOf("${{listOf<Unbound>()}}");
                     val other = arrayOf("clean")
                     replace(&other[0], "concrete")
                     {operation}
                     return 0 }}"#
            ),
            &[operation, "items[0]"],
            &raw,
            &final_diagnostics,
            Some(r#"replace(&other[0], "concrete")"#),
        );
    }
}

#[test]
fn mutable_storage_reads_follow_reachable_replacements() {
    let declarations = "fun keep(value: Int): Int = value\nfun flag(): Boolean = true\nfun maybe(): Int? = null\nfun absent(): Nothing? = null";
    let pending = r#"listOf("${listOf<Unbound>()}")"#;
    let clean = "listOf<String>()";
    for (initial, update, blocked) in [
        (pending, String::new(), true),
        (pending, format!("if (flag()) {{ items = {clean} }}"), true),
        (clean, format!("if (flag()) {{ items = {pending} }}"), true),
        (pending, format!("items = {clean}"), false),
        (
            pending,
            format!("items = if (flag()) {{ items = {clean}; {clean} }} else {clean}"),
            false,
        ),
        (
            clean,
            format!("items = if (flag()) {{ val discarded = {pending}; return 0 }} else {clean}"),
            false,
        ),
        (
            clean,
            format!(
                "items = when (flag()) {{ true -> {{ val discarded = {pending}; return 0 }}; else -> {clean} }}"
            ),
            false,
        ),
        (
            pending,
            format!("if (flag()) {{ items = {clean} }} else {{ items = {clean} }}"),
            false,
        ),
        (
            pending,
            format!("if (flag()) {{ items = {clean} }} else {{ return 0 }}"),
            false,
        ),
        (
            clean,
            format!("if (flag()) {{ items = {pending}; return 0 }}"),
            false,
        ),
        (
            pending,
            format!(
                "when (flag()) {{ true -> {{ items = {clean} }}; else -> {{ items = {clean} }} }}"
            ),
            false,
        ),
        (
            pending,
            format!(
                "true && (if (flag()) {{ items = {clean}; true }} else {{ items = {clean}; false }})"
            ),
            false,
        ),
        (
            pending,
            format!(
                "flag() && (if (flag()) {{ items = {clean}; true }} else {{ items = {clean}; false }})"
            ),
            true,
        ),
        (
            clean,
            format!("false && (if (flag()) {{ items = {pending}; true }} else false)"),
            false,
        ),
        (
            pending,
            format!(
                "absent() ?: (if (flag()) {{ items = {clean}; 1 }} else {{ items = {clean}; 2 }})"
            ),
            false,
        ),
        (
            pending,
            format!(
                "maybe() ?: (if (flag()) {{ items = {clean}; 1 }} else {{ items = {clean}; 2 }})"
            ),
            true,
        ),
    ] {
        let entry = format!("{{ var items = {initial}\n{update}\nreturn keep(items.size) }}");
        let rejected: &[_] = if blocked {
            &["keep(items.size)", "items.size", "listOf<Unbound>()"]
        } else {
            &["listOf<Unbound>()"]
        };
        let diagnostics: &[_] = if initial == clean && blocked {
            &[("L0165", "Unbound"), ("L0165", "items")]
        } else {
            &[("L0165", "Unbound")]
        };
        assert_call_recovery_with_raw(
            declarations,
            &entry,
            rejected,
            &[],
            diagnostics,
            (!blocked).then_some("keep(items.size)"),
        );
    }
}

#[test]
fn mutable_replacement_tracks_only_reachable_when_selectors() {
    let choose = "choose({ ignored -> val captured = text; 1 })";
    let assignment =
        format!("items = when {{ {choose} -> listOf<String>(); else -> listOf<String>() }}");
    assert_call_recovery_with_raw(
        "fun choose(cb: (borrow Int) -> Int): Boolean = true\n\
         fun choose(cb: (borrow String) -> String): Boolean = false\n\
         fun keep(value: Int): Int = value",
        &format!(
            r#"{{ val text = "${{listOf<Unbound>()}}";
                 var items = listOf<String>()
                 {assignment}
                 return keep(items.size) }}"#
        ),
        &["keep(items.size)", "items.size", &assignment],
        &[],
        &[("L0165", "Unbound"), ("L0165", choose), ("L0165", "items")],
        None,
    );
    assert_call_recovery_with_raw(
        "fun flag(): Boolean = true\nfun keep(value: Int): Int = value",
        r#"{ val dirty = listOf("${listOf<Unbound>()}");
             var items = listOf<String>()
             items = when (flag()) {
                 true, false, dirty.size > 0 -> listOf<String>()
                 else -> listOf<String>()
             }
             return keep(items.size) }"#,
        &["listOf<Unbound>()"],
        &[],
        &[("L0165", "Unbound")],
        Some("keep(items.size)"),
    );
}

#[test]
fn mutable_storage_reads_follow_loop_backedges_and_jumps() {
    let declarations = "fun flag(): Boolean = true\nfun keep(value: Int): Int = value";
    let pending = r#"listOf("${listOf<Unbound>()}")"#;
    let backedge_diagnostics = &[("L0165", "items"), ("L0165", "items"), ("L0165", "Unbound")];
    let exit_diagnostics = &[("L0165", "Unbound"), ("L0165", "items")];
    for (body, diagnostics) in [
        (
            format!("while (flag()) {{ keep(items.size); items = {pending} }}; return 0"),
            backedge_diagnostics.as_slice(),
        ),
        (
            format!(
                "while (flag()) {{ keep(items.size); if (flag()) {{ items = {pending}; continue }} }}; return 0"
            ),
            backedge_diagnostics.as_slice(),
        ),
        (
            format!(
                "while (flag()) {{ if (flag()) {{ items = {pending}; break }} }}; return keep(items.size)"
            ),
            exit_diagnostics.as_slice(),
        ),
        (
            format!(
                "for (n in listOf<Int>(1)) {{ items = {pending}; continue }}; return keep(items.size)"
            ),
            &[("L0165", "items"), ("L0165", "Unbound"), ("L0165", "items")],
        ),
        (
            format!(
                "loop {{ while (flag()) {{ items = {pending}; break }}; break }}; return keep(items.size)"
            ),
            exit_diagnostics.as_slice(),
        ),
    ] {
        let entry = format!("{{ var items = listOf<String>(); {body} }}");
        assert_call_recovery_with_raw(
            declarations,
            &entry,
            &["keep(items.size)", "items.size"],
            &[],
            diagnostics,
            None,
        );
    }
    for (initial, body) in [
        (
            "listOf<String>()",
            format!("loop {{ break; items = {pending}; break }}"),
        ),
        (
            "listOf<String>()",
            format!("loop {{ if (true) {{ break }}; items = {pending}; break }}"),
        ),
        (
            "listOf<String>()",
            format!("loop {{ if (true) {{ break }} else {{ 0 }}; items = {pending}; break }}"),
        ),
        (
            pending,
            "loop { items = listOf<String>(); break }".to_owned(),
        ),
        (
            "listOf<String>()",
            format!(
                "loop {{ false && (if (flag()) {{ items = {pending}; break }} else false); break }}"
            ),
        ),
        (
            "listOf<String>()",
            format!(
                "loop {{ when (flag()) {{ true, false -> 0; else -> {{ items = {pending}; break }} }}; break }}"
            ),
        ),
        (
            "listOf<String>()",
            format!("while (false) {{ items = {pending} }}"),
        ),
        (
            "listOf<String>()",
            format!(
                "while (if (flag()) {{ items = {pending}; true }} else {{ items = listOf<String>(); false }}) {{ return 0 }}"
            ),
        ),
        (
            "listOf<String>()",
            format!(
                "while ((if (flag()) {{ items = {pending}; true }} else {{ items = listOf<String>(); false }}) && true) {{ return 0 }}"
            ),
        ),
    ] {
        let entry = format!("{{ var items = {initial}; {body}; return keep(items.size) }}");
        assert_call_recovery_with_raw(
            declarations,
            &entry,
            &["listOf<Unbound>()"],
            &[],
            &[("L0165", "Unbound")],
            Some("keep(items.size)"),
        );
    }
    for statement in [
        format!("loop {{ continue }}; items = {pending}"),
        format!("while (true) {{}}; items = {pending}"),
        format!("if (false) {{ items = {pending} }}"),
    ] {
        let entry = format!(
            "{{ var items = listOf<String>(); while (flag()) {{ keep(items.size); {statement} }}; return 0 }}"
        );
        assert_call_recovery_with_raw(
            declarations,
            &entry,
            &["listOf<Unbound>()"],
            &[],
            &[("L0165", "Unbound")],
            Some("keep(items.size)"),
        );
    }
    let entry = format!(
        "{{ var items = listOf<String>(); var buffer = listOf<String>(); while (flag()) {{ keep(items.size); keep(1); items = buffer; buffer = {pending} }}; return 0 }}"
    );
    assert_call_recovery_with_raw(
        declarations,
        &entry,
        &["keep(items.size)", "items.size"],
        &[],
        &[
            ("L0165", "items"),
            ("L0165", "items"),
            ("L0165", "buffer"),
            ("L0165", "buffer"),
            ("L0165", "Unbound"),
        ],
        Some("keep(1)"),
    );
}

#[test]
fn repeated_explicit_external_type_correlates_distinct_parameters() {
    assert_call_recovery(
        "fun <T, U> take(value: Pair<T, U>): Int = 0",
        "take<Unbound, Unbound>(Pair<Int, Boolean>(1, true))",
        Some(("L0084", "Pair<Int, Boolean>(1, true)")),
        &[
            ("L0165", "Unbound"),
            ("L0165", "Unbound"),
            ("L0084", "Pair<Int, Boolean>(1, true)"),
        ],
        Some("Pair<Int, Boolean>(1, true)"),
    );
    assert_call_recovery(
        "fun <T, U> take(first: T, second: U): Int = 0",
        "take<Unbound, Unbound>(1, 2)",
        None,
        &[("L0165", "Unbound"), ("L0165", "Unbound")],
        None,
    );
    assert_call_recovery(
        "fun <T, U> take(value: Pair<T, U>, flag: Int): Int = 0\n\
         fun <T, U> take(value: Pair<T, U>, flag: Long): Int = 1",
        "take<Unbound, Unbound>(Pair<Int, Boolean>(1, true), 1)",
        Some(("L0123", "take")),
        &[
            ("L0123", "take"),
            ("L0165", "Unbound"),
            ("L0165", "Unbound"),
        ],
        Some("Pair<Int, Boolean>(1, true)"),
    );
}

#[test]
fn explicit_external_type_identity_survives_nested_type_references() {
    let declarations =
        "class Phantom<T>(val value: Int)\nfun <T, U> take(first: T, second: U): Int = 0";
    for (call, conflict, external) in [
        (
            "take<Carrier<Unbound>, Unbound>(Carrier<Int>(1), true)",
            Some(("L0084", "true")),
            "Unbound",
        ),
        (
            "take<Carrier<Unbound>, Other>(Carrier<Int>(1), true)",
            None,
            "Other",
        ),
        (
            "take<Carrier<Unbound>, Unbound>(Carrier<Int>(1), 1)",
            None,
            "Unbound",
        ),
        (
            "take<List<Phantom<Unbound>>, Unbound>(listOf<Phantom<Int>>(Phantom<Int>(1)), true)",
            Some(("L0084", "true")),
            "Unbound",
        ),
        (
            "take<List<Unbound>, Unbound>(listOf<Int>(1), true)",
            Some(("L0084", "true")),
            "Unbound",
        ),
    ] {
        let mut diagnostics = vec![("L0165", "Unbound"), ("L0165", external)];
        diagnostics.extend(conflict);
        assert_call_recovery(declarations, call, conflict, &diagnostics, None);
    }
    for (actual, conflict) in [("true", Some(("L0084", "true"))), ("1", None)] {
        let call = format!("take<Unbound?, Unbound>(value, {actual})");
        let mut diagnostics = vec![("L0165", "Unbound?"), ("L0165", "Unbound")];
        diagnostics.extend(conflict);
        assert_call_recovery_in_entry(
            declarations,
            &format!("{{ val value: Int? = null; return {call} }}"),
            &call,
            conflict,
            &diagnostics,
            None,
        );
    }
    assert_call_recovery(
        "fun <T> take(value: T): Int = 0",
        "take<Carrier<Unbound?>>(Carrier<Int>(1))",
        Some(("L0084", "Carrier<Int>(1)")),
        &[("L0165", "Unbound?"), ("L0084", "Carrier<Int>(1)")],
        Some("Carrier<Int>(1)"),
    );
    assert_call_recovery(
        "fun <T> take(value: T): Int = 0",
        "take<Pair<Unbound, Unbound>>(Pair<Int, Boolean>(1, true))",
        Some(("L0084", "Pair<Int, Boolean>(1, true)")),
        &[
            ("L0165", "Unbound"),
            ("L0165", "Unbound"),
            ("L0084", "Pair<Int, Boolean>(1, true)"),
        ],
        Some("Pair<Int, Boolean>(1, true)"),
    );
    for (actual, conflict) in [("true", Some(("L0084", "true"))), ("1", None)] {
        let call = format!("take<(borrow Unbound) -> Unbound, Unbound>(callback, {actual})");
        let mut diagnostics = vec![("L0165", "Unbound"); 3];
        diagnostics.extend(conflict);
        assert_call_recovery_in_entry(
            declarations,
            &format!("{{ val callback: (borrow Int) -> Int = {{ item -> item }}; return {call} }}"),
            &call,
            conflict,
            &diagnostics,
            None,
        );
    }
    assert_call_recovery_in_entry(
        "fun <T, U> repeat(first: T, second: U): Int =\n\
         repeat<Pair<T, Unbound>, Unbound>(Pair<T, Int>(first, 1), 1)",
        "= 0",
        "repeat<Pair<T, Unbound>, Unbound>(Pair<T, Int>(first, 1), 1)",
        None,
        &[("L0165", "Unbound"), ("L0165", "Unbound")],
        Some("Pair<T, Int>(first, 1)"),
    );
    for (parameter, conflict) in [("Int", Some(("L0084", "callback"))), ("X", None)] {
        let call = "take<(borrow X) -> Unbound>(callback)";
        let declarations = format!(
            "fun <T> take(value: T): Int = 0\n\
             fun <X> caller(callback: (borrow {parameter}) -> Int): Int = {call}"
        );
        let mut diagnostics = vec![("L0165", "Unbound")];
        diagnostics.extend(conflict);
        assert_call_recovery_in_entry(&declarations, "= 0", call, conflict, &diagnostics, None);
    }
}

#[test]
fn pending_explicit_function_type_checks_lambda_header_without_committing_its_body() {
    for (lambda, conflict) in [
        ("{ item -> inside() }", None),
        ("{ first, second -> inside() }", Some(("L0084", "->"))),
    ] {
        let call = format!("take<(borrow Unbound) -> Unbound>({lambda})");
        let mut diagnostics = vec![("L0165", "Unbound"); 2];
        diagnostics.extend(conflict);
        assert_call_recovery(
            "fun <T> take(callback: T): Int = 0",
            &call,
            conflict,
            &diagnostics,
            None,
        );
    }
}

#[test]
fn repeated_explicit_external_type_respects_contextual_integer_literals() {
    let declarations = "fun <T, U> take(fixed: Carrier<T>, value: U): Int = 0";
    for call in [
        "take<Unbound, Unbound>(Carrier<Long>(1L), 1)",
        "take<Unbound, Unbound>(Carrier<Byte>(0), (-128))",
        "take<Unbound, Unbound>(Carrier<ULong>(1uL), 1u)",
        "take<Unbound, Unbound>(Carrier<Long>(1L), if (true) 1 else 2)",
        "take<Unbound, Unbound>(Carrier<List<Long>>(listOf<Long>(1L)), listOf(1))",
    ] {
        assert_call_recovery(
            declarations,
            call,
            None,
            &[("L0165", "Unbound"), ("L0165", "Unbound")],
            None,
        );
    }
    assert_call_recovery(
        "fun <T, U> take(fixed: Carrier<T>, values: List<U>): Int = 0",
        "take<Unbound, Unbound>(Carrier<Long>(1L), listOf(1))",
        None,
        &[("L0165", "Unbound"), ("L0165", "Unbound")],
        None,
    );
    for (entry, call, primary) in [
        (
            "= take<Unbound, Unbound>(Carrier<Byte>(0), -129)",
            "take<Unbound, Unbound>(Carrier<Byte>(0), -129)",
            "-129",
        ),
        (
            "{ val value: Int = 1; return take<Unbound, Unbound>(Carrier<Long>(1L), value) }",
            "take<Unbound, Unbound>(Carrier<Long>(1L), value)",
            "value",
        ),
        (
            "= take<Unbound, Unbound>(Carrier<UInt>(1u), 1)",
            "take<Unbound, Unbound>(Carrier<UInt>(1u), 1)",
            "1",
        ),
    ] {
        assert_call_recovery_in_entry(
            declarations,
            entry,
            call,
            Some(("L0084", primary)),
            &[
                ("L0165", "Unbound"),
                ("L0165", "Unbound"),
                ("L0084", primary),
            ],
            None,
        );
    }
}
