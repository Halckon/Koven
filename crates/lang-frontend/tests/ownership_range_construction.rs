//! 新范围 descriptor 对实际 owning List 根的持有、root-flat 派生与终止。
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        BorrowBindingStorage, CompilationUnitOwnership, UnitLoanTarget,
        check_compilation_unit_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

// Primary single-document consumer uses source-qualified Unit facts.
fn checked(
    body: &str,
) -> (
    SourceMap,
    CompilationUnitOwnership,
    CompilationUnitOwnership,
) {
    let mut sources = SourceMap::new();
    let text = format!(
        "fun observe(source: View<String>): Unit {{}}\nfun alias(source: View<String>): borrow View<String> from source = source\nfun consume(own source: List<String>): Unit {{}}\n{body}"
    );
    let source = sources.add_source("range-construct.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, source).unwrap();
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let names = names.validate().unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let typed = typed.validate().unwrap();
    let single =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    let inputs = [SourceUnitInput::new(
        "std",
        "range-construct.ko",
        source,
        &parsed,
    )];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    (sources, single, unit)
}

#[test]
fn constructing_a_range_continues_the_real_root_loan_and_ends_before_move() {
    let (_, memory, rooted) = checked(
        "fun run(): Unit { val source = listOf(\"kept\"); borrow val part = rangeView(source, 0, 1); observe(part); consume(source) }",
    );
    for owned in [memory, rooted] {
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let [binding] = owned.borrow_results().bindings() else {
            panic!("new descriptor binding")
        };
        assert_eq!(binding.storage(), BorrowBindingStorage::NewRangeDescriptor);
        assert!(binding.parent().is_none());
        let UnitLoanTarget::Place(root) = binding.origin() else {
            panic!("owning root")
        };
        assert_ne!(root.root(), binding.binding());
        let continuation = binding.source_loan().unwrap();
        let loan = owned
            .loans()
            .iter()
            .find(|loan| {
                loan.call() == continuation.call() && loan.argument() == continuation.argument()
            })
            .unwrap();
        assert_eq!(loan.target(), binding.origin());
        assert_eq!(
            owned.borrow_results().ends()[0].binding(),
            binding.binding()
        );
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn range_iteration_keeps_root_and_noncopyable_element_read_only_in_both_unit_layouts() {
    for (body, code) in [
        (
            "fun run():Unit{val source=listOf(\"kept\");borrow val part=rangeView(source,0,1);for(item in part){consume(source)}}",
            "L0135",
        ),
        (
            "class Item(val n:Int){}\nfun consumeItem(own item:Item):Unit{}\nfun run(view:View<Item>):Unit{for(item in view){consumeItem(item)}}",
            "L0133",
        ),
    ] {
        let (_, memory, rooted) = checked(body);
        for diagnostics in [memory.diagnostics(), rooted.diagnostics()] {
            assert!(
                !diagnostics.is_empty(),
                "illegal move through a View provider was accepted"
            );
            assert!(
                diagnostics.iter().any(|d| d.code().to_string() == code),
                "{diagnostics:?}"
            );
        }
    }
}

#[test]
fn range_descriptor_iteration_never_publishes_an_owned_metadata_drop() {
    use lang_frontend::ownership_checking::UnitDropTarget;
    let (_, memory, rooted) = checked(
        "fun run():Unit{val source=listOf(\"kept\");block {borrow val part=rangeView(source,0,1);for(item in part){println(item)}};consume(source)}",
    );
    for owned in [memory, rooted] {
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let binding = owned.borrow_results().bindings()[0].binding();
        assert!(
            !owned
                .drops()
                .iter()
                .any(|fact| fact.target() == UnitDropTarget::Named(binding))
        );
        let [iteration] = owned.iterations() else {
            panic!("the range loop must publish its iteration provider");
        };
        assert_eq!(
            iteration.descriptor().provider(),
            lang_frontend::type_checking::IterationProvider::RangeView
        );
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn temporary_range_provider_continues_the_actual_root_in_both_unit_layouts() {
    let (sources, memory, rooted) =
        checked("fun run():Unit{for(item in rangeView(listOf(\"temporary\"),0,1)){println(item)}}");
    for owned in [memory, rooted] {
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let [fact] = owned.borrow_results().range_uses() else {
            panic!("range continuation");
        };
        let UnitLoanTarget::Temporary(root) = *fact.origin() else {
            panic!("actual temporary root");
        };
        assert_ne!(root, fact.expression());
        assert_eq!(owned.iterations()[0].source(), Some(fact.origin()));
        assert_eq!(
            sources
                .slice(
                    owned
                        .loans()
                        .iter()
                        .find(|loan| loan.call() == fact.source_loan().call()
                            && loan.argument() == fact.source_loan().argument())
                        .unwrap()
                        .begin_span()
                )
                .unwrap(),
            "listOf(\"temporary\")"
        );
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn nested_view_construction_continues_the_original_temporary_collection_root() {
    let (sources, memory, rooted) = checked(
        "fun run():Unit{for(item in rangeView(rangeView(listOf(\"a\".clone()),0,1),0,1)){println(item)}}",
    );
    for owned in [memory, rooted] {
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        assert_eq!(owned.borrow_results().range_uses().len(), 2);
        for fact in owned.borrow_results().range_uses() {
            let UnitLoanTarget::Temporary(root) = *fact.origin() else {
                panic!("actual collection root");
            };
            assert_eq!(
                sources
                    .slice(
                        owned
                            .loans()
                            .iter()
                            .find(|loan| loan.argument() == root)
                            .unwrap()
                            .begin_span()
                    )
                    .unwrap(),
                "listOf(\"a\".clone())"
            );
        }
        assert_eq!(
            owned.iterations()[0].source(),
            Some(owned.borrow_results().range_uses()[0].origin())
        );
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn immediate_range_borrow_keeps_named_root_protected_through_later_arguments() {
    let (_, single, unit) = checked(
        "fun read(view:View<String>,own removed:List<String>):Unit{}\nfun run():Unit{val source=listOf(\"kept\");read(rangeView(source,0,1),source)}",
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(
            diagnostics.iter().any(|d| d.code().to_string() == "L0135"),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn conditional_call_does_not_enable_general_borrow_binding_last_use() {
    let (_, single, unit) = checked(
        "fun read(view:View<String>,number:Int):Unit{}\nfun run(flag:Boolean):Unit{val root=listOf(\"kept\");borrow val parent=rangeView(root,0,1);read(rangeView(parent,0,1),if(flag){return}else{0});consume(root)}",
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(
            diagnostics.iter().any(|d| d.code().to_string() == "L0135"),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn immediate_range_use_and_loop_restore_named_root_permissions() {
    for body in [
        "fun run():Unit{val source=listOf(\"kept\");observe(rangeView(source,0,1));consume(source)}",
        "fun run():Unit{val source=listOf(\"kept\");for(item in rangeView(source,0,1)){println(item)};consume(source)}",
    ] {
        let (_, single, unit) = checked(body);
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    }
}

#[test]
fn temporary_range_cannot_escape_into_binding_or_return() {
    for body in [
        "fun run():Unit{borrow val saved=rangeView(listOf(\"temporary\"),0,1);observe(saved)}",
        "fun run():Unit{borrow val saved=alias(rangeView(listOf(\"temporary\"),0,1));observe(saved)}",
        "fun escape(source:List<String>):View<String> from source=rangeView(listOf(\"temporary\"),0,1)",
    ] {
        let (_, single, unit) = checked(body);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert!(
                diagnostics.iter().any(|d| d.code().to_string() == "L0162"),
                "{diagnostics:?}"
            );
        }
        assert!(single.borrow_results().range_uses().is_empty());
        assert!(unit.borrow_results().range_uses().is_empty());
    }
}

#[test]
fn live_range_or_sibling_keeps_the_root_protected() {
    for body in [
        "fun run(): Unit { val source = listOf(\"kept\"); borrow val part = rangeView(source, 0, 1); consume(source); observe(part) }",
        "fun run():Unit{val source=listOf(\"kept\");borrow val parent=rangeView(source,0,1);borrow val child=rangeView(parent,0,1);consume(source);observe(child)}",
        "fun run(): Unit { val source = listOf(\"kept\"); borrow val left = rangeView(source, 0, 1); borrow val right = rangeView(source, 0, 0); observe(left); consume(source); observe(right) }",
    ] {
        let (_, single, unit) = checked(body);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert!(
                diagnostics.iter().any(|d| d.code().to_string() == "L0135"),
                "{diagnostics:?}"
            );
        }
    }
}

#[test]
fn derived_descriptor_inherits_root_without_parent_metadata_and_alias_keeps_metadata() {
    let (_, single, unit) = checked(
        "fun run(): Unit { val source = listOf(\"kept\"); borrow val parent = rangeView(source, 0, 1); borrow val child = rangeView(parent, 0, 1); borrow val metadata = alias(child); observe(metadata); consume(source) }",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    let [parent, child, metadata] = single.borrow_results().bindings() else {
        panic!("three bindings")
    };
    assert_eq!(parent.storage(), BorrowBindingStorage::NewRangeDescriptor);
    assert_eq!(child.storage(), BorrowBindingStorage::NewRangeDescriptor);
    assert!(child.parent().is_none());
    assert_eq!(
        metadata.storage(),
        BorrowBindingStorage::BorrowedCarrierMetadata
    );
    assert_eq!(metadata.parent(), Some(child.binding()));
    assert_eq!(parent.origin(), child.origin());
    assert_eq!(child.origin(), metadata.origin());
    assert_eq!(
        single
            .borrow_results()
            .ends()
            .iter()
            .map(|e| e.binding())
            .collect::<Vec<_>>(),
        [parent.binding(), metadata.binding(), child.binding()]
    );
    let [parent, child, metadata] = unit.borrow_results().bindings() else {
        panic!("three unit bindings")
    };
    assert_eq!(parent.storage(), BorrowBindingStorage::NewRangeDescriptor);
    assert_eq!(child.storage(), BorrowBindingStorage::NewRangeDescriptor);
    assert!(child.parent().is_none());
    assert_eq!(
        metadata.storage(),
        BorrowBindingStorage::BorrowedCarrierMetadata
    );
    assert_eq!(metadata.parent(), Some(child.binding()));
    assert_eq!(parent.origin(), child.origin());
    assert_eq!(child.origin(), metadata.origin());
    assert_eq!(
        unit.borrow_results()
            .ends()
            .iter()
            .map(|e| e.binding())
            .collect::<Vec<_>>(),
        [parent.binding(), metadata.binding(), child.binding()]
    );
}
#[test]
fn range_statement_if_preserves_lexical_borrow_ends() {
    use lang_frontend::ownership_checking::UnitDropPoint;

    let (_, single, unit) = checked(
        r#"
fun run(flag:Boolean):Unit{
    val source=listOf("kept")
    borrow val part=rangeView(source,0,1)
    if(flag){observe(part)}else{observe(part)}
    observe(part)
    consume(source)
}
"#,
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0135"),
            "statement-if must not invent general last-use borrow ending: {diagnostics:?}"
        );
    }

    let (_, single, unit) = checked(
        r#"
fun run(flag:Boolean):Unit{
    val source=listOf("kept")
    block {
        borrow val part=rangeView(source,0,1)
        if(flag){observe(part)}else{observe(part)}
        observe(part)
    }
    consume(source)
}
"#,
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }
    let [binding] = single.borrow_results().bindings() else {
        panic!("one source-produced descriptor binding")
    };
    assert_eq!(binding.storage(), BorrowBindingStorage::NewRangeDescriptor);
    assert!(binding.source_loan().is_some());
    let [end] = single.borrow_results().ends() else {
        panic!("the descriptor has one lexical-scope end, not arm or last-read ends")
    };
    assert_eq!(end.binding(), binding.binding());
    assert!(
        matches!(end.point(), UnitDropPoint::AfterStatement(_)),
        "{:?}",
        end.point()
    );

    let [binding] = unit.borrow_results().bindings() else {
        panic!("one source-qualified descriptor binding")
    };
    assert_eq!(binding.storage(), BorrowBindingStorage::NewRangeDescriptor);
    assert!(binding.source_loan().is_some());
    let [end] = unit.borrow_results().ends() else {
        panic!("the unit descriptor has one lexical-scope end")
    };
    assert_eq!(end.binding(), binding.binding());
    assert!(
        matches!(end.point(), UnitDropPoint::AfterStatement(_)),
        "{:?}",
        end.point()
    );
    assert!(single.deferred().is_empty(), "{:?}", single.deferred());
    assert!(unit.validate().is_ok());
}

#[test]
fn ordinary_borrow_result_inside_range_statement_if_remains_unproven() {
    let (sources, single, unit) = checked(
        r#"
fun scalar(source:Int):borrow Int from source=source
fun readInt(value:Int):Unit{}
fun run(flag:Boolean,n:Int):Unit{
    val source=listOf("kept")
    block {
        borrow val part=rangeView(source,0,1)
        if(flag){readInt(scalar(n))}else{observe(part)}
        observe(part)
    }
    consume(source)
}
"#,
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code().to_string() == "L0164")
            .expect("the caller continuation is not proved by a local if");
        assert_eq!(
            sources.slice(diagnostic.primary_span()).unwrap(),
            "scalar(n)"
        );
    }
}

#[test]
fn terminated_range_prefix_releases_source_before_outer_loop_exit() {
    for consumer in [
        "for (item in rangeView(source, BOUND, 1)) {}",
        "observe(rangeView(source, BOUND, 1))",
    ] {
        for bound in [
            "break",
            "if (flag) { break } else { 0 }",
            "continue",
            "if (flag) { continue } else { 0 }",
        ] {
            let body = consumer.replace("BOUND", bound);
            let text = format!(
                "fun run(own source: List<String>, flag: Boolean): Unit {{ loop {{ {body}; break }}; consume(source) }}"
            );
            let (_, document, rooted) = checked(&text);
            for owned in [&document, &rooted] {
                assert!(
                    owned.diagnostics().is_empty(),
                    "{text}: {:?}",
                    owned.diagnostics()
                );
                if matches!(bound, "break" | "continue") {
                    assert!(owned.borrow_results().range_uses().is_empty(), "{text}");
                    assert!(owned.iterations().is_empty(), "{text}");
                    assert!(owned.borrow_results().bindings().is_empty(), "{text}");
                }
            }
        }
    }
}

#[test]
fn completed_range_prefix_still_protects_live_source() {
    for body in [
        "for (item in rangeView(source, 0, 1)) { consume(source) }",
        "borrow val part = rangeView(source, 0, 1); consume(source); observe(part)",
        "for (item in rangeView(source, 0, 1)) { loop { observe(rangeView(source, break, 1)); break }; consume(source) }",
    ] {
        let text = format!("fun run(own source: List<String>): Unit {{ {body} }}");
        let (_, document, rooted) = checked(&text);
        for owned in [&document, &rooted] {
            assert!(
                owned
                    .diagnostics()
                    .iter()
                    .any(|d| d.code().to_string() == "L0135"),
                "{text}: {:?}",
                owned.diagnostics()
            );
        }
    }
}

#[test]
fn interrupted_range_binding_keeps_existing_loop_boundary() {
    let (_, document, rooted) = checked(
        "fun run(own source: List<String>): Unit { loop { borrow val part = rangeView(source, break, 1); observe(part); break }; consume(source) }",
    );
    for owned in [&document, &rooted] {
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0164"),
            "{:?}",
            owned.diagnostics()
        );
    }
}

#[test]
fn grouped_range_arguments_keep_actual_call_identity_and_root_lifetime() {
    for depth in [0, 1, 3] {
        for source in ["source", "listOf(\"temporary\")"] {
            let range = format!("rangeView({source}, 0, 1)");
            let argument = format!("{}{range}{}", "(".repeat(depth), ")".repeat(depth));
            let text = format!(
                "fun run(own source: List<String>): Unit {{ observe({argument}); consume(source) }}"
            );
            let (sources, document, rooted) = checked(&text);
            for owned in [&document, &rooted] {
                assert!(
                    owned.diagnostics().is_empty(),
                    "{text}: {:?}",
                    owned.diagnostics()
                );
                let [fact] = owned.borrow_results().range_uses() else {
                    panic!("one actual range use: {text}")
                };
                let source_loan = owned
                    .loans()
                    .iter()
                    .find(|loan| {
                        loan.call() == fact.source_loan().call()
                            && loan.argument() == fact.source_loan().argument()
                    })
                    .expect("original actual source loan");
                assert_eq!(fact.expression(), source_loan.call());
                assert_eq!(source_loan.target(), fact.origin());
                assert_eq!(sources.slice(source_loan.begin_span()), Ok(source));
                assert!(matches!(
                    fact.site(),
                    lang_frontend::ownership_checking::RangeUseSite::Call(_)
                ));
            }
        }
    }
}

#[test]
fn grouped_range_arguments_preserve_prefix_exits_and_live_root_rejection() {
    for bound in [
        "break",
        "if (flag) { break } else { 0 }",
        "if (flag) { continue } else { 0 }",
    ] {
        let text = format!(
            "fun run(own source: List<String>, flag: Boolean): Unit {{ loop {{ observe(((rangeView(source, {bound}, 1)))); break }}; consume(source) }}"
        );
        let (_, document, rooted) = checked(&text);
        for owned in [&document, &rooted] {
            assert!(
                owned.diagnostics().is_empty(),
                "{text}: {:?}",
                owned.diagnostics()
            );
            if bound == "break" {
                assert!(owned.borrow_results().range_uses().is_empty());
            }
        }
    }
    let (_, document, rooted) = checked(
        "fun use(view: View<String>, own removed: List<String>): Unit {}\nfun run(own source: List<String>): Unit { use(((rangeView(source, 0, 1))), source) }",
    );
    for owned in [&document, &rooted] {
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0135"),
            "{:?}",
            owned.diagnostics()
        );
    }
}

#[test]
fn grouped_nested_range_source_keeps_root_protected_in_later_arguments() {
    for depth in [0, 1, 3] {
        let source = format!(
            "{}rangeView(root, 0, 1){}",
            "(".repeat(depth),
            ")".repeat(depth)
        );
        let text = format!(
            "fun move(own value: List<String>): Int = 1\nfun read(view: View<String>, number: Int): Unit {{}}\nfun run(): Unit {{ val root = listOf(\"kept\"); read(rangeView({source}, 0, 1), move(root)) }}"
        );
        let (sources, document, rooted) = checked(&text);
        for owned in [&document, &rooted] {
            let conflict = owned
                .diagnostics()
                .iter()
                .find(|d| d.code().to_string() == "L0135")
                .unwrap_or_else(|| {
                    panic!(
                        "live root must remain protected: {text}: {:?}",
                        owned.diagnostics()
                    )
                });
            assert_eq!(sources.slice(conflict.primary_span()), Ok("root"));
        }
    }
}

#[test]
fn grouped_nested_range_source_keeps_real_root_identity_and_cleanup() {
    use lang_frontend::ownership_checking::{RangeUseSite, UnitDropPoint, UnitDropTarget};
    for depth in [0, 1, 3] {
        for temporary in [false, true] {
            let root = if temporary {
                "listOf(\"kept\")"
            } else {
                "root"
            };
            let source = format!(
                "{}rangeView({root}, 0, 1){}",
                "(".repeat(depth),
                ")".repeat(depth)
            );
            let setup = if temporary {
                ""
            } else {
                "val root = listOf(\"kept\");"
            };
            let tail = if temporary { "" } else { "consume(root)" };
            let text =
                format!("fun run(): Unit {{ {setup} observe(rangeView({source}, 0, 1)); {tail} }}");
            let (sources, document, rooted) = checked(&text);
            for owned in [&document, &rooted] {
                assert!(
                    owned.diagnostics().is_empty(),
                    "{text}: {:?}",
                    owned.diagnostics()
                );
                let [inner, outer] = owned.borrow_results().range_uses() else {
                    panic!("two actual ranges: {text}")
                };
                assert_eq!(inner.origin(), outer.origin(), "{text}");
                for (fact, operand) in [(inner, root), (outer, source.as_str())] {
                    let from = fact.source_loan();
                    let loan = owned
                        .loans()
                        .iter()
                        .find(|loan| {
                            loan.call() == from.call() && loan.argument() == from.argument()
                        })
                        .unwrap();
                    assert_eq!(loan.target(), inner.origin());
                    assert_eq!(sources.slice(loan.begin_span()), Ok(operand));
                }
                if temporary {
                    let UnitLoanTarget::Temporary(root) = inner.origin() else {
                        panic!("actual temporary root")
                    };
                    let RangeUseSite::Call(consumer) = outer.site() else {
                        panic!("outer consumer")
                    };
                    let cleanup: Vec<_> = owned
                        .drops()
                        .iter()
                        .filter(|drop| drop.target() == UnitDropTarget::Temporary(*root))
                        .collect();
                    assert_eq!(cleanup.len(), 1);
                    assert_eq!(cleanup[0].point(), UnitDropPoint::CallReturn(consumer));
                } else {
                    assert!(matches!(inner.origin(), UnitLoanTarget::Place(_)));
                }
            }
        }
    }
}

#[path = "ownership_range_construction/grouped_iteration.rs"]
mod grouped_iteration;

// The planner pending token is not the checker's active binding loan.
// Check both Unit layouts; this is behavior, not a complete end table.
#[test]
fn metadata_child_protects_root_write_until_its_last_use() {
    for (tail, reject) in [
        ("root=listOf(\"changed\");observe(child)", true),
        ("observe(child);root=listOf(\"changed\")", false),
    ] {
        let body = format!(
            "fun run():Unit{{var root=listOf(\"kept\");borrow val parent=rangeView(root,0,1);borrow val child=alias(parent);{tail}}}"
        );
        let (_, document, rooted) = checked(&body);
        for diagnostics in [document.diagnostics(), rooted.diagnostics()] {
            let codes = diagnostics
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>();
            assert_eq!(
                codes,
                if reject {
                    vec!["L0135".to_owned()]
                } else {
                    vec![]
                },
                "{tail}"
            );
        }
        for unit in [document, rooted] {
            if reject {
                assert!(unit.borrow_results().bindings().is_empty());
                assert!(unit.loans().is_empty());
                assert!(unit.drops().is_empty());
                assert!(unit.validate().is_err());
            } else {
                assert!(unit.deferred().is_empty());
                let [parent, child] = unit.borrow_results().bindings() else {
                    panic!("two real bindings")
                };
                assert_eq!(child.parent(), Some(parent.binding()));
                assert_eq!(child.origin(), parent.origin());
                assert_eq!(
                    unit.borrow_results()
                        .ends()
                        .iter()
                        .map(|end| end.binding())
                        .collect::<Vec<_>>(),
                    [child.binding(), parent.binding()]
                );
                assert!(unit.validate().is_ok());
            }
        }
    }
}
