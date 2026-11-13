//! SPEC-0296: ordinary-source last-use reads precede owned nullable cleanup.
use super::resource_exchange_tests::run_unit;
use crate::native_tests::{boxed_enum_tests::assert_success, emit_link_and_run};

const KNOWN_BOUNDARY: &str =
    include_str!("../../tests/fixtures/map-word-frequency-0295/known-boundary/src/p/provider.ko");

pub(super) fn run_single(source: &str, body: &str, expected: &str) {
    let source = format!("{source}\nfun main(): Unit {{ {body} }}");
    let run = emit_link_and_run("owned-null-cleanup.ko", &source, "main");
    assert_success(&run, expected.as_bytes());
}

pub(super) fn run_units(source: &str, body: &str, expected: &str) {
    let provider = format!("package p\n{source}");
    let consumer = format!("package q\nfun entry(): Unit {{ {body} }}");
    for constants in [false, true] {
        let (run, _) = run_unit(&provider, &consumer, constants);
        assert_success(&run, expected.as_bytes());
    }
}

#[test]
fn unit_owned_null_cleanup_known_loop_boundary() {
    run_units(
        KNOWN_BOUNDARY.strip_prefix("package p\n").unwrap(),
        "p.exercise(arrayOf(\"key\", \"key\"))",
        "text present\nkey\nkey\ndone\n",
    );
}

const LAST_USE: &str = r#"
fun exercise(): Unit {
    var texts = mutableMapOf<Int, String>()
    texts.put(1, "value".clone())
    val value = texts.remove(1)
    if (value != null) { println("present") } else { error("missing") }
    val absent = texts.remove(1)
    if (absent == null) { println("missing") } else { error("present") }
}
"#;

#[test]
fn single_owned_null_cleanup_without_loop() {
    run_single(LAST_USE, "exercise()", "present\nmissing\n");
}

#[test]
fn unit_owned_null_cleanup_without_loop() {
    run_units(LAST_USE, "p.exercise()", "present\nmissing\n");
}

#[test]
fn single_owned_null_cleanup_scalar_last_read() {
    run_single(&scalar_last_read(), "exercise()", "present\nmissing\n");
}

#[test]
fn unit_owned_null_cleanup_scalar_last_read() {
    run_units(&scalar_last_read(), "p.exercise()", "present\nmissing\n");
}

fn scalar_last_read() -> String {
    LAST_USE
        .replace(
            "if (value != null)",
            "val present = value != null\n    if (present)",
        )
        .replace(
            "if (absent == null)",
            "val missing = absent == null\n    if (missing)",
        )
}

fn comparison_forms() -> (String, String) {
    let mut source = "value class Text(val text: String)\n".to_owned();
    let mut calls = String::new();
    let mut expected = String::new();
    for (index, condition) in [
        "value != null",
        "null != value",
        "((value)) != null",
        "value == null",
        "null == (value)",
        "((value)) == null",
    ]
    .iter()
    .enumerate()
    {
        for (label, ty, payload) in [
            ("string", "String", "\"text\".clone()"),
            ("inline", "Text", "Text(\"text\".clone())"),
        ] {
            let absent = condition.replace("value", "absent");
            source.push_str(&format!(
                "fun {label}{index}(): Unit {{\n var m = mutableMapOf<Int, {ty}>()\n m.put(1, {payload})\n val value = m.remove(1)\n val present = {condition}\n if (present) {{ println(\"yes\") }} else {{ println(\"no\") }}\n val absent = m.remove(1)\n val missing = {absent}\n if (missing) {{ println(\"yes\") }} else {{ println(\"no\") }}\n}}\n"
            ));
            calls.push_str(&format!("{label}{index}(); "));
            expected.push_str(if index < 3 { "yes\nno\n" } else { "no\nyes\n" });
        }
    }
    source.push_str(&format!("fun exercise(): Unit {{ {calls} }}"));
    (source, expected)
}

#[test]
fn single_owned_null_cleanup_grouped_and_mirrored_reads() {
    let (source, expected) = comparison_forms();
    run_single(&source, "exercise()", &expected);
}

#[test]
fn unit_owned_null_cleanup_grouped_and_mirrored_reads() {
    let (source, expected) = comparison_forms();
    run_units(&source, "p.exercise()", &expected);
}

const REUSE_AND_CONSUME: &str = r#"
class Resource(val text: String) { deinit() { println(this.text) } }
fun consume(own value: Resource): Unit { println("consume") }
fun exercise(): Unit {
    var texts = mutableMapOf<Int, String>()
    texts.put(1, "text".clone())
    val value = texts.remove(1)
    val present = value != null
    if (!present) { error("missing text") }
    val missing = null == value
    if (missing) { error("lost text") }
    println(value!!)
    var resources = mutableMapOf<Int, Resource>()
    resources.put(1, Resource("consumed"))
    resources.put(2, Resource("lexical"))
    val consumed = resources.remove(1)
    val available = consumed != null
    if (!available) { error("missing consumed resource") }
    consume(consumed!!)
    val lexical = resources.remove(2)
    val isPresent = lexical != null
    if (!isPresent) { error("missing resource") }
    println("before scope exit")
}
"#;

#[test]
fn single_owned_null_cleanup_preserves_reuse_consumption_and_lexical_resource() {
    run_single(
        REUSE_AND_CONSUME,
        "exercise()",
        "text\nconsume\nconsumed\nbefore scope exit\nlexical\n",
    );
}

#[test]
fn unit_owned_null_cleanup_preserves_reuse_consumption_and_lexical_resource() {
    run_units(
        REUSE_AND_CONSUME,
        "p.exercise()",
        "text\nconsume\nconsumed\nbefore scope exit\nlexical\n",
    );
}

#[test]
fn unit_owned_null_cleanup_loop_resource_exits() {
    for (exit, expected) in [
        (
            "",
            "present\nbody\ntail\none\nbody\ntail\npresent\nbody\ntail\ntwo\nafter\ndone\n",
        ),
        (
            "continue",
            "present\nbody\none\nbody\npresent\nbody\ntwo\nafter\ndone\n",
        ),
        ("break", "present\nbody\none\nafter\ntwo\ndone\n"),
        ("return", "present\nbody\none\ntwo\ndone\n"),
    ] {
        let source = format!(
            r#"
class Resource(val text: String) {{ deinit() {{ println(this.text) }} }}
fun exercise(args: Array<Int>): Unit {{
    var resources = mutableMapOf<Int, Resource>()
    resources.put(1, Resource("one"))
    resources.put(2, Resource("two"))
    for (key in args) {{
        val removed = resources.remove(key)
        if (removed != null) {{ println("present") }}
        println("body")
        {exit}
        println("tail")
    }}
    println("after")
}}
"#
        );
        run_units(
            &source,
            "p.exercise(arrayOf(1, 1, 2)); println(\"done\")",
            expected,
        );
    }
}

#[test]
fn unit_owned_null_cleanup_zero_and_one_iteration() {
    run_units(
        KNOWN_BOUNDARY.strip_prefix("package p\n").unwrap(),
        "p.exercise(arrayOf<String>()); p.exercise(arrayOf(\"key\"))",
        "done\ntext present\nkey\ndone\n",
    );
}

pub(crate) mod iteration_resources {
    //! Owner identity and observable cleanup order across iteration and pending call frames.
    use super::super::resource_exchange_tests::run_unit;
    use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations};

    const LEAF: &str = "class Leaf(val name: String) { deinit() { println(this.name) } }";

    #[test]
    fn unit_for_resource_elements_and_local_owners_drop_in_lexical_order() {
        for factory in ["arrayOf", "listOf", "mutableListOf"] {
            for (exit, visits) in [("", 2), ("continue", 2), ("break", 1), ("return", 1)] {
                let provider = format!(
                    "package p\n{LEAF}\nfun inspect(leaf: Leaf): Unit {{ println(\"visit\") }}\nfun work(): Unit {{ val outer = Leaf(\"outer\"); for (item in {factory}(Leaf(\"first\"), Leaf(\"second\"))) {{ val local = Leaf(\"local\"); inspect(item); {exit} }}; println(\"after\") }}"
                );
                let consumer = "package q\nfun entry(): Unit { p.work(); println(\"caller\") }";
                let expected = format!(
                    "{}second\nfirst\n{}outer\ncaller\n",
                    "visit\nlocal\n".repeat(visits),
                    if exit == "return" { "" } else { "after\n" }
                );
                for constants in [false, true] {
                    let (run, ir) = run_unit(&provider, consumer, constants);
                    assert_success(&run, expected.as_bytes());
                    assert_success(
                        &run_counted_allocations(&ir, 4 + visits),
                        expected.as_bytes(),
                    );
                }
            }
        }
    }

    #[test]
    fn unit_for_pending_borrow_call_cleans_prefix_before_provider() {
        for exit in ["continue", "break", "return"] {
            let provider = format!(
                "package p\n{LEAF}\nfun sink(leaf: Leaf, own flag: Boolean): Unit {{ println(\"committed\") }}\nfun work(own flag: Boolean): Unit {{ val outer = Leaf(\"outer\"); for (_ in arrayOf(Leaf(\"element\"))) {{ val local = Leaf(\"local\"); sink(Leaf(\"prefix\"), if(flag) {{ {exit} }} else {{ true }}); println(\"body-after\") }}; println(\"after\") }}"
            );
            let consumer =
                "package q\nfun entry(): Unit { p.work(true); p.work(false); println(\"caller\") }";
            let expected = format!(
                "prefix\nlocal\nelement\n{}outer\ncommitted\nprefix\nbody-after\nlocal\nelement\nafter\nouter\ncaller\n",
                if exit == "return" { "" } else { "after\n" }
            );
            for constants in [false, true] {
                let (run, ir) = run_unit(&provider, consumer, constants);
                assert_success(&run, expected.as_bytes());
                assert_success(&run_counted_allocations(&ir, 10), expected.as_bytes());
            }
        }
    }

    #[test]
    fn unit_for_outer_pending_call_waits_until_inner_provider_cleanup() {
        let provider = format!(
            r#"package p
{LEAF}
fun sink(leaf: Leaf, own flag: Boolean): Unit {{ println("unexpected call") }}
fun work(own flag: Boolean): Unit {{
    sink(Leaf("prefix"), if(flag) {{
        for (_ in arrayOf(Leaf("element"))) {{ val local = Leaf("local"); return }}
        true
    }} else {{ true }})
    println("unexpected after")
}}
"#
        );
        let consumer = "package q\nfun entry(): Unit { p.work(true); println(\"caller\") }";
        for constants in [false, true] {
            let (run, ir) = run_unit(&provider, consumer, constants);
            assert_success(&run, b"local\nelement\nprefix\ncaller\n");
            assert_success(
                &run_counted_allocations(&ir, 4),
                b"local\nelement\nprefix\ncaller\n",
            );
        }
    }

    #[test]
    fn unit_for_pending_receiver_and_argument_preserve_distinct_loan_identities() {
        let provider = format!(
            r#"package p
{LEAF}
class Host(val leaf: Leaf) {{
    fun sink(other: Leaf, own flag: Boolean): Unit {{ println("unexpected call") }}
    deinit() {{ println("receiver") }}
}}
fun work(own flag: Boolean): Unit {{
    for (_ in arrayOf(Leaf("element"))) {{
        Host(Leaf("receiver field")).sink(Leaf("argument"), if(flag) {{ return }} else {{ true }})
    }}
    println("unexpected after")
}}
"#
        );
        let consumer = "package q\nfun entry(): Unit { p.work(true); println(\"caller\") }";
        for constants in [false, true] {
            let (run, ir) = run_unit(&provider, consumer, constants);
            let expected = b"argument\nreceiver\nreceiver field\nelement\ncaller\n";
            assert_success(&run, expected);
            assert_success(&run_counted_allocations(&ir, 5), expected);
        }
    }

    #[test]
    fn unit_for_abort_does_not_unwind_element_source_or_pending_owners() {
        let provider = format!(
            r#"package p
{LEAF}
fun sink(leaf: Leaf, own flag: Boolean): Unit {{ println("unexpected call") }}
fun work(): Unit {{
    val outer = Leaf("unexpected outer drop")
    for (_ in arrayOf(Leaf("unexpected element drop"))) {{
        val local = Leaf("unexpected local drop")
        println("before abort")
        sink(Leaf("unexpected prefix drop"), error("stop"))
    }}
    println("unexpected after")
}}
"#
        );
        let consumer = "package q\nfun entry(): Unit { p.work(); println(\"unexpected caller\") }";
        for constants in [false, true] {
            let (run, _) = run_unit(&provider, consumer, constants);
            assert!(!run.status.success(), "{run:?}");
            assert_eq!(run.stdout, b"before abort\n");
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(run.status.signal(), Some(6));
            }
        }
    }

    #[test]
    fn unit_for_element_receiver_and_argument_reuse_the_callers_loan() {
        let provider = r#"package p
class Host(val name: String) {
    fun inspect(other: Host, own flag: Boolean): Unit { println("unexpected call") }
    deinit() { println(this.name) }
}
fun work(own flag: Boolean): Unit {
    for (host in arrayOf(Host("element"))) {
        host.inspect(host, if(flag) { return } else { true })
    }
    println("unexpected after")
}"#;
        let consumer = "package q\nfun entry(): Unit { p.work(true); println(\"caller\") }";
        for constants in [false, true] {
            let (run, ir) = run_unit(provider, consumer, constants);
            assert_success(&run, b"element\ncaller\n");
            assert_success(&run_counted_allocations(&ir, 2), b"element\ncaller\n");
        }
    }

    #[test]
    fn unit_for_pending_field_replace_keeps_previous_owner_and_ends_exclusive_loan() {
        let provider = format!(
            r#"package p
{LEAF}
class Holder(var state: Leaf) {{ deinit() {{ println("parent") }} }}
fun work(own flag: Boolean): Unit {{
    for (_ in listOf(1)) {{
        val holder = Holder(Leaf("old"))
        val previous = replace(&holder.state, if(flag) {{ return }} else {{ Leaf("new") }})
        println("committed")
    }}
    println("after")
}}
"#
        );
        let consumer =
            "package q\nfun entry(): Unit { p.work(true); p.work(false); println(\"caller\") }";
        let expected = b"parent\nold\ncommitted\nold\nparent\nnew\nafter\ncaller\n";
        for constants in [false, true] {
            let (run, ir) = run_unit(&provider, consumer, constants);
            assert_success(&run, expected);
            assert_success(
                &crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(
                    &ir,
                    &[1, 2, 0, 4, 6, 5, 3],
                ),
                expected,
            );
        }
    }

    #[test]
    fn unit_for_resource_value_components_return_cleans_nested_sources_once() {
        let provider = format!(
            "package p\n{LEAF}\nvalue class Parts(val number: Int, val leaf: Leaf, val text: String)\nfun inspect(leaf: Leaf): Unit {{ println(\"inspect\") }}\nfun work(): Int {{ for ((number, leaf, text) in arrayOf(Parts(7, Leaf(\"outer element\"), \"text\"))) {{ for (_ in listOf(Leaf(\"inner element\"))) {{ inspect(leaf); println(text); return number }} }}; return 0 }}"
        );
        let consumer = "package q\nfun entry(): Unit { if(p.work() != 7) { error(\"bad copy return\") }; println(\"caller\") }";
        let expected = b"inspect\ntext\ninner element\nouter element\ncaller\n";
        for constants in [false, true] {
            let (run, ir) = run_unit(&provider, consumer, constants);
            assert_success(&run, expected);
            assert_success(
                &crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(
                    &ir,
                    &[2, 3, 0, 1],
                ),
                expected,
            );
        }
    }

    #[test]
    fn unit_qualified_while_reassignment_releases_replaced_string_owners_once() {
        let provider = r#"
package p
value class Pair(val first: Int, val second: Int)
fun fresh(left: String, right: String): String = left + right
fun work(limit: Int): Unit {
    val (second, first) = Pair(20, 22)
    var outer: String? = fresh("fi", "rst")
    val untouched: String? = fresh("ke", "pt")
    var step = 0
    if (outer != null && untouched != null) {
        while (step < limit && first == 20) {
            step = step + 1
            outer = fresh("sec", "ond")
            if (step < limit) { continue }
            break
        }
        println(untouched)
    }
    if (outer != null && outer == "second") { println("changed") }
}
"#;
        let consumer = "package q\nfun entry(): Unit { p.work(0); p.work(3) }";
        let expected = b"kept\nkept\nchanged\n";
        for constants in [false, true] {
            let (run, ir) = run_unit(provider, consumer, constants);
            assert_success(&run, expected);
            // Two fresh roots per call and one fresh replacement per executed iteration.
            assert_success(&run_counted_allocations(&ir, 2 + 2 + 3), expected);
        }
    }
}

pub(crate) mod sequential_iteration {
    //! Real native provider matrix; each output checks continuation as well as element order.
    use super::super::resource_exchange_tests::run_unit;
    use crate::native_tests::boxed_enum_tests::assert_success;

    #[test]
    fn unit_for_all_providers_sources_and_cardinalities() {
        for (container, factory) in [
            ("Array", "arrayOf"),
            ("List", "listOf"),
            ("MutableList", "mutableListOf"),
        ] {
            for (elements, visits, count) in [
                ("", "", 0),
                ("\"one\"", "one\n", 1),
                ("\"one\", \"二\", \"three\"", "one\n二\nthree\n", 3),
            ] {
                for source in ["owned", "borrow", "temporary"] {
                    let provider = format!(
                        "package p\nfun make(): {container}<String> {{ println(\"source\"); return {factory}<String>({elements}) }}\nfun visit(xs: {container}<String>): Unit {{ for (item in xs) {{ println(item) }}; println(\"visit-end\") }}"
                    );
                    let body = match source {
                        "owned" => {
                            "val xs = p.make(); for (item in xs) { println(item) }; for (_ in xs) { println(\"reuse\") }"
                        }
                        "borrow" => {
                            "val xs = p.make(); p.visit(xs); for (_ in xs) { println(\"reuse\") }"
                        }
                        _ => "for (item in p.make()) { println(item) }",
                    };
                    let consumer =
                        format!("package q\nfun entry(): Unit {{ {body}; println(\"after\") }}");
                    let expected = format!(
                        "source\n{visits}{}{}after\n",
                        if source == "borrow" {
                            "visit-end\n"
                        } else {
                            ""
                        },
                        if source == "temporary" {
                            String::new()
                        } else {
                            "reuse\n".repeat(count)
                        }
                    );
                    for constants in [false, true] {
                        let (run, _) = run_unit(&provider, &consumer, constants);
                        assert_success(&run, expected.as_bytes());
                    }
                }
            }
        }
    }

    #[test]
    fn unit_for_control_edges_preserve_borrowed_owner_and_caller() {
        for (container, factory) in [
            ("Array", "arrayOf"),
            ("List", "listOf"),
            ("MutableList", "mutableListOf"),
        ] {
            for source in ["owned", "borrow", "temporary"] {
                for exit in ["normal", "continue", "break", "return"] {
                    let head = match source {
                        "owned" => "val xs = make(); for (item in xs)",
                        "borrow" => "for (item in xs)",
                        _ => "for (item in make())",
                    };
                    let transfer = match exit {
                        "normal" => "",
                        "continue" => "continue",
                        "break" => "break",
                        _ => "return",
                    };
                    let parameter = if source == "borrow" {
                        format!("xs: {container}<String>")
                    } else {
                        String::new()
                    };
                    let provider = format!(
                        "package p\nfun make(): {container}<String> {{ println(\"source\"); return {factory}(\"one\", \"two\", \"three\") }}\nfun visit({parameter}): Unit {{ {head} {{ println(item); {transfer} }}; println(\"after\") }}"
                    );
                    let caller = if source == "borrow" {
                        "val xs = p.make(); p.visit(xs); for (_ in xs) { println(\"reuse\") }"
                    } else {
                        "p.visit()"
                    };
                    let consumer =
                        format!("package q\nfun entry(): Unit {{ {caller}; println(\"caller\") }}");
                    let expected = format!(
                        "source\n{}{}{}caller\n",
                        if ["normal", "continue"].contains(&exit) {
                            "one\ntwo\nthree\n"
                        } else {
                            "one\n"
                        },
                        if exit == "return" { "" } else { "after\n" },
                        if source == "borrow" {
                            "reuse\nreuse\nreuse\n"
                        } else {
                            ""
                        }
                    );
                    for constants in [false, true] {
                        let (run, _) = run_unit(&provider, &consumer, constants);
                        assert_success(&run, expected.as_bytes());
                    }
                }
            }
        }
    }

    #[test]
    fn unit_for_cross_file_value_class_borrowed_components_keep_elements() {
        let provider = "package p\nvalue class Pair(val text: String, val code: Int)\nfun pairs(): List<Pair> = listOf(Pair(\"one\", 1), Pair(\"two\", 2))";
        let consumer = "package q\nfun entry(): Unit { val xs = p.pairs(); for ((text, code) in xs) { println(text); if (code == 1) { continue }; println(\"second\") }; for ((text, _) in xs) { println(text) }; for ((_, _) in xs) { println(\"discard\") }; println(\"after\") }";
        for constants in [false, true] {
            let (run, _) = run_unit(provider, consumer, constants);
            assert_success(
                &run,
                b"one\ntwo\nsecond\none\ntwo\ndiscard\ndiscard\nafter\n",
            );
        }
    }

    #[test]
    fn unit_for_nested_for_and_while_keep_nearest_loop_target() {
        let provider = "package p\nfun values(): List<String> = listOf(\"one\", \"two\")";
        let consumer = r#"package q
fun entry(): Unit {
    for (outer in p.values()) {
        println(outer)
        var once = true
        while (once) { once = false; for (inner in p.values()) { println(inner); break }; continue }
        for (inner in p.values()) { println(inner); continue }
        println("outer-end")
    }
    println("after")
}"#;
        for constants in [false, true] {
            let (run, _) = run_unit(provider, consumer, constants);
            assert_success(
                &run,
                b"one\none\none\ntwo\nouter-end\ntwo\none\none\ntwo\nouter-end\nafter\n",
            );
        }
    }

    #[test]
    fn unit_for_source_return_happens_before_provider_acquisition() {
        let provider = "package p\nfun early(): Unit { for (x in if (true) { return } else { arrayOf(1) }) { println(\"unexpected\") }; println(\"unexpected after\") }";
        let consumer = "package q\nfun entry(): Unit { p.early(); println(\"caller\") }";
        for constants in [false, true] {
            let (run, _) = run_unit(provider, consumer, constants);
            assert_success(&run, b"caller\n");
        }
    }

    #[test]
    fn unit_for_source_conditional_return_preserves_source_evaluation_order() {
        let provider = "package p\nfun make(): List<String> { println(\"source\"); return listOf(\"item\") }\nfun work(own flag: Boolean): Unit { for (item in if(flag) { println(\"return-source\"); return } else { make() }) { println(item) }; println(\"after\") }";
        let consumer =
            "package q\nfun entry(): Unit { p.work(true); p.work(false); println(\"caller\") }";
        for constants in [false, true] {
            let (run, _) = run_unit(provider, consumer, constants);
            assert_success(&run, b"return-source\nsource\nitem\nafter\ncaller\n");
        }
    }

    #[test]
    fn unit_for_source_abort_happens_before_provider_acquisition() {
        let provider = "package p\nfun work(): Unit { for (item in if(true) { println(\"abort-source\"); error(\"stop\") } else { listOf(1) }) { println(\"unexpected\") }; println(\"unexpected after\") }";
        let consumer = "package q\nfun entry(): Unit { p.work(); println(\"unexpected caller\") }";
        for constants in [false, true] {
            let (run, _) = run_unit(provider, consumer, constants);
            assert_eq!(run.stdout, b"abort-source\n");
            assert!(!run.status.success(), "{run:?}");
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(run.status.signal(), Some(6));
            }
        }
    }

    #[test]
    fn unit_for_snapshots_length_once_before_the_guard_and_uses_provider_primitives() {
        let analysis = super::super::analyze_sources(
            "package p\nfun source(): Array<Int> = arrayOf(1, 2, 3)",
            "package q\nfun entry(): Unit { for (item in p.source()) { if(item == 2) { continue }; println(\"visit\") } }",
        );
        let (program, entry) = super::super::lower_scalar_unit_with_entry(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
        )
        .unwrap();
        let function = program.modules[0].function(entry).unwrap();
        let snapshots = function
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(
                    instruction.operation,
                    crate::ssa::model::Operation::ContainerLength { .. }
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            snapshots.len(),
            1,
            "each acquired provider snapshots its length once"
        );
        let preheader = function.block(snapshots[0].block).unwrap();
        assert_eq!(Some(preheader.id), function.entry_block());
        let crate::ssa::model::TerminatorKind::Branch(edge) =
            &preheader.terminator.as_ref().unwrap().kind
        else {
            panic!("snapshot preheader must enter the guard")
        };
        assert!(matches!(
            function
                .block(edge.target)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .kind,
            crate::ssa::model::TerminatorKind::Conditional { .. }
        ));
        let body_allocations = function
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(
                    instruction.operation,
                    crate::ssa::model::Operation::ContainerConstruct { .. }
                        | crate::ssa::model::Operation::HeapAllocate { .. }
                )
            })
            .count();
        assert_eq!(
            body_allocations, 0,
            "provider state must not allocate an iterator object"
        );
    }
}

pub(crate) mod resource_exchange_tests {
    //! Cross-file resource exchange through both public native unit entries.

    use super::super::*;
    use lang_frontend::ownership_checking::check_compilation_unit_constant_ownership;

    const PROVIDER: &str = r#"package p
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
    fn unit_resource_exchange_native_commit_preserves_lexical_owners_and_unique_frees() {
        let consumer = r#"package q
import p.Leaf
import p.Guard
fun entry(): Unit {
    var first = Guard("old body", Leaf("old first"), Leaf("old second"))
    var second = Guard("second body", Leaf("second first"), Leaf("second second"))
    val old = replace(&first, p.replacement())
    println("after replace")
    p.consume(old)
    swap(&first, &second)
    println("after swap")
}
"#;
        let expected = b"evaluate new\nafter replace\nconsume old\nold body\nold second\nold first\nafter swap\nnew body\nnew second\nnew first\nsecond body\nsecond second\nsecond first\n";
        for constants in [false, true] {
            let (run, ir) = run_unit(PROVIDER, consumer, constants);
            crate::native_tests::boxed_enum_tests::assert_success(&run, expected);
            let counted = crate::native_tests::boxed_enum_tests::run_counted_allocations(&ir, 9);
            crate::native_tests::boxed_enum_tests::assert_success(&counted, expected);
        }
    }

    #[test]
    fn unit_resource_exchange_native_returned_and_borrowed_old_owner_drop_once() {
        let provider = r#"package p
class Leaf(val name: String) { deinit() { println(this.name) } }
fun takeOld(): Leaf {
    var target = Leaf("new root")
    return replace(&target, Leaf("replacement root"))
}
fun inspect(leaf: Leaf): Unit { println("inspect old") }
"#;
        let consumer = r#"package q
import p.Leaf
fun entry(): Unit {
    val returned = p.takeOld()
    println("received old")
    var target = Leaf("borrowed old")
    p.inspect(replace(&target, Leaf("borrowed replacement")))
    println("after inspect")
}
"#;
        for constants in [false, true] {
            let (run, _) = run_unit(provider, consumer, constants);
            crate::native_tests::boxed_enum_tests::assert_success(
            &run,
            b"replacement root\nreceived old\ninspect old\nborrowed old\nafter inspect\nborrowed replacement\nnew root\n",
        );
        }
    }

    #[test]
    fn unit_resource_exchange_native_pending_exit_keeps_old_root_initialized() {
        for exit in ["return", "break", "continue"] {
            let provider = format!(
                r#"package p
class Leaf(val name: String) {{ deinit() {{ println(this.name) }} }}
fun exercise(own flag: Boolean): Unit {{
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
"#
            );
            let consumer = "package q\nfun entry(): Unit { p.exercise(true); p.exercise(false); println(\"done\") }";
            // The base unit entry retains its existing argument-control-transfer boundary.
            let (run, _) = run_unit(&provider, consumer, true);
            let expected = if exit == "return" {
                "old\ncommitted\nold\nafter loop\nnew\ndone\n"
            } else {
                "after loop\nold\ncommitted\nold\nafter loop\nnew\ndone\n"
            };
            crate::native_tests::boxed_enum_tests::assert_success(&run, expected.as_bytes());
        }
    }

    #[test]
    fn unit_resource_exchange_native_abort_does_not_run_deinit() {
        let provider = "package p\nclass Leaf { deinit() { println(\"unexpected deinit\") } }";
        let consumer = r#"package q
import p.Leaf
fun entry(): Unit {
    var target = Leaf()
    println("before abort")
    val old = replace(&target, error("stop"))
    println("unexpected commit")
}
"#;
        for constants in [false, true] {
            let (run, _) = run_unit(provider, consumer, constants);
            assert!(!run.status.success(), "{run:?}");
            assert_eq!(run.stdout, b"before abort\n");
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(run.status.signal(), Some(6), "{run:?}");
            }
        }
    }

    pub(crate) fn run_unit(
        provider: &str,
        consumer: &str,
        constants: bool,
    ) -> (std::process::Output, String) {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", provider);
        let (consumer_source, consumer) = parsed(&mut sources, "q/consumer.ko", consumer);
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let (name_environment, environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .unwrap()
            .validate()
            .unwrap();
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == "entry")
            .unwrap()
            .id();
        let result = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
        let directory = TestDirectory::create();
        let object = directory.join("resource-exchange.o");
        let executable = directory.join("resource-exchange");
        let mut first_ir = None;
        let mut first_object = None;
        if constants {
            let typed = result
                .validate_constants()
                .expect("constant-capable resource types");
            let owned = check_compilation_unit_constant_ownership(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
            )
            .unwrap()
            .validate()
            .expect("constant-capable resource ownership");
            for order in [inputs, [inputs[1], inputs[0]]] {
                let (program, function) =
                    crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
                        &sources,
                        &order,
                        &names,
                        &environment,
                        &typed,
                        &owned,
                        entry,
                    )
                    .expect("constant unit resource SSA");
                let ir = crate::llvm::render_verified_program_with_entry(&program, function)
                    .expect("constant unit resource LLVM");
                crate::emit_native_constant_unit_object(
                    &sources,
                    &order,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                    entry,
                    &object,
                )
                .expect("public constant unit object");
                assert_stable_output(ir, &object, &mut first_ir, &mut first_object);
            }
        } else {
            let typed = result.validate().expect("resource types");
            let owned =
                check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
                    .unwrap()
                    .validate()
                    .expect("resource ownership");
            for order in [inputs, [inputs[1], inputs[0]]] {
                let (program, function) = lower_scalar_unit_with_entry(
                    &sources,
                    &order,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                    entry,
                )
                .expect("unit resource SSA");
                let ir = crate::llvm::render_verified_program_with_entry(&program, function)
                    .expect("unit resource LLVM");
                emit_native_unit_object(
                    &sources,
                    &order,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                    entry,
                    &object,
                )
                .expect("public unit object");
                assert_stable_output(ir, &object, &mut first_ir, &mut first_object);
            }
        }
        assert_no_sibling_temporary(&directory.0);
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&executable).output().unwrap();
        (run, first_ir.expect("lowered unit LLVM"))
    }

    fn assert_stable_output(
        ir: String,
        object: &Path,
        first_ir: &mut Option<String>,
        first_object: &mut Option<Vec<u8>>,
    ) {
        if let Some(first) = first_ir {
            crate::test_support::diagnostics::assert_text_eq!(first, &ir);
        } else {
            *first_ir = Some(ir);
        }
        let bytes = fs::read(object).unwrap();
        if let Some(first) = first_object {
            assert_eq!(first, &bytes);
        } else {
            *first_object = Some(bytes);
        }
    }
}
