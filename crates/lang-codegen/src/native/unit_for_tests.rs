//! Real native provider matrix; each output checks continuation as well as element order.
use super::resource_exchange_tests::run_unit;
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
    let analysis = super::analyze_sources(
        "package p\nfun source(): Array<Int> = arrayOf(1, 2, 3)",
        "package q\nfun entry(): Unit { for (item in p.source()) { if(item == 2) { continue }; println(\"visit\") } }",
    );
    let (program, entry) = super::lower_scalar_unit_with_entry(
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
