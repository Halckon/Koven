//! A direct borrow binding's receiver child ends without inventing a payload address.
use super::receiver_return::NODE;
use super::*;
use lang_frontend::{
    ownership_checking::{
        BorrowBindingStorage, UnitReceiverOwnershipFact, UnitReceiverOwnershipKind,
        UnitReceiverOwnershipTarget,
    },
    type_checking::UnitCallReceiverOrigin,
};

struct BindingCall<'a> {
    call: &'a str,
    operand: &'a str,
    source: &'a str,
}
fn end<'a>(call: &'a str, operand: &'a str, source: &'a str) -> BindingCall<'a> {
    BindingCall {
        call,
        operand,
        source,
    }
}

fn assert_binding_receiver(
    observed: &Observed,
    entry: usize,
    receiver: &UnitReceiverOwnershipFact,
    expected: &BindingCall<'_>,
) {
    let unit = &observed.units[entry];
    let names = &observed.unit_names[entry];
    let [source] = names.names().index().source_units() else {
        panic!("one source")
    };
    assert_eq!(source.source_id(), observed.parsed.source_id());
    match (entry, source.key()) {
        (0, SourceUnitKey::Rooted { root, logical_path }) => {
            assert_eq!(root.as_str(), "root");
            assert_eq!(logical_path.as_str(), "call_ends.ko");
        }
        (1, SourceUnitKey::MemoryDocument) => {}
        other => panic!("unexpected source: {other:?}"),
    }
    let node = |id: UnitExpressionId| {
        assert_eq!(id.source_unit(), source.id());
        observed
            .parsed
            .ast()
            .expressions()
            .get(id.expression())
            .unwrap()
    };
    let symbol_name = |symbol: lang_frontend::name_resolution::UnitSymbolId| {
        assert_eq!(symbol.source_unit(), source.id());
        names.names().source_units()[symbol.source_unit().index()]
            .resolution()
            .symbols()[symbol.symbol().index()]
        .name()
    };
    assert!(unit.receiver_facts().contains(receiver));
    assert_eq!(receiver.kind(), UnitReceiverOwnershipKind::SharedLoan);
    assert!(!receiver.is_receiver_reservation());
    assert_eq!(
        observed
            .sources
            .slice(node(receiver.call()).span())
            .unwrap(),
        expected.call
    );
    assert_eq!(receiver.end_span(), node(receiver.call()).span());
    let UnitCallReceiverOrigin::Expression(operand) = receiver.source() else {
        panic!("explicit binding receiver")
    };
    assert_eq!(
        observed.sources.slice(node(operand).span()).unwrap(),
        expected.operand
    );
    assert_eq!(receiver.begin_span(), node(operand).span());
    let UnitReceiverOwnershipTarget::Place(place) = receiver.target() else {
        panic!("canonical source lease")
    };
    assert!(place.is_root());
    assert_eq!(symbol_name(place.root()), expected.source);
    let binding_name = expected.operand.trim_matches(['(', ')']);
    let binding = unit
        .borrow_results()
        .bindings()
        .iter()
        .find(|b| symbol_name(b.binding()) == binding_name)
        .unwrap();
    assert_ne!(
        binding.binding(),
        place.root(),
        "operand binding is not the source lease root"
    );
    assert_eq!(binding.origin(), &UnitLoanTarget::Place(place.clone()));
    assert_eq!(binding.storage(), BorrowBindingStorage::BorrowedStorage);
    assert!(binding.parent().is_none());
    assert!(binding.source_loan().is_none());
    assert_eq!(
        observed
            .sources
            .slice(node(binding.initializer()).span())
            .unwrap()
            .trim_matches(['(', ')']),
        expected.source
    );
    assert!(
        unit.drops()
            .iter()
            .all(|drop| drop.target() != UnitDropTarget::Named(binding.binding())),
        "the borrow binding never owns the payload"
    );
}

type Prefix<'a> = (BindingCall<'a>, &'a str, usize, usize);
fn assert_binding_ends(
    text: &str,
    normal: &[BindingCall<'_>],
    prefixes: &[Prefix<'_>],
) -> Observed {
    let observed = observe(text);
    assert_units_admitted(&observed);
    for (entry, unit) in observed.units.iter().enumerate() {
        assert_eq!(
            unit.receiver_call_return_ends().len(),
            normal.len(),
            "complete normal vector: {text}"
        );
        assert_eq!(
            unit.receiver_call_prefix_ends().len(),
            prefixes.len(),
            "complete prefix vector: {text}"
        );
        for (event, expected) in unit.receiver_call_return_ends().iter().zip(normal) {
            assert_binding_receiver(&observed, entry, event.receiver(), expected);
            assert_eq!(
                event.point(),
                UnitDropPoint::CallReturn(event.receiver().call())
            );
        }
        for (event, (expected, jump, calls, receivers)) in
            unit.receiver_call_prefix_ends().iter().zip(prefixes)
        {
            assert_binding_receiver(&observed, entry, event.receiver(), expected);
            let UnitDropPoint::ControlTransfer(transfer) = event.point() else {
                panic!("actual jump")
            };
            assert_eq!(
                transfer.source_unit(),
                event.receiver().call().source_unit()
            );
            let node = observed
                .parsed
                .ast()
                .expressions()
                .get(transfer.expression())
                .unwrap();
            assert_eq!(observed.sources.slice(node.span()).unwrap(), *jump);
            assert_eq!(event.preceding_drops(), 0);
            assert_eq!(event.preceding_call_loan_ends(), *calls);
            assert_eq!(event.preceding_receiver_loan_ends(), *receivers);
        }
    }
    observed
}

#[test]
fn direct_binding_normal_completion_ends_the_actual_child_receiver() {
    assert_binding_ends(
        &format!(
            "{NODE}fun run(own node: Node): Unit {{ borrow val borrowed = node; borrowed.touch(\"direct\", 0) }}"
        ),
        &[end("borrowed.touch(\"direct\", 0)", "borrowed", "node")],
        &[],
    );
}

#[test]
fn direct_binding_return_prefix_ends_the_actual_child_receiver() {
    assert_binding_ends(
        &format!(
            "{NODE}fun run(own node: Node): Unit {{ borrow val borrowed = node; borrowed.touch(\"direct\", return) }}"
        ),
        &[],
        &[(
            end("borrowed.touch(\"direct\", return)", "borrowed", "node"),
            "return",
            1,
            0,
        )],
    );
}

#[test]
fn grouped_direct_bindings_keep_source_identity_for_all_parameter_modes() {
    for mode in ["own ", "", "borrow ", "inout "] {
        for (open, close, next) in [
            ("", "", "0"),
            ("", "", "return"),
            ("loop {", "}", "break"),
            ("while (flag) {", "}", "continue"),
        ] {
            let call = format!("((borrowed)).touch(\"group\", {next})");
            let text = format!(
                "{NODE}fun run({mode}node: Node, flag: Boolean): Unit {{ borrow val borrowed = ((node)); {open} {call} {close} }}"
            );
            let observed = if next == "0" {
                assert_binding_ends(&text, &[end(&call, "((borrowed))", "node")], &[])
            } else {
                assert_binding_ends(
                    &text,
                    &[],
                    &[(end(&call, "((borrowed))", "node"), next, 1, 0)],
                )
            };
            if mode != "own " {
                for unit in &observed.units {
                    let UnitLoanTarget::Place(origin) =
                        unit.borrow_results().bindings()[0].origin()
                    else {
                        panic!("source lease")
                    };
                    assert!(
                        unit.drops()
                            .iter()
                            .all(|drop| drop.target() != UnitDropTarget::Named(origin.root()))
                    );
                }
            }
        }
    }
}

#[test]
fn child_completion_does_not_end_a_parent_binding_with_a_later_use() {
    let first = "borrowed.touch(\"first\", 0)";
    let last = "borrowed.touch(\"last\", 0)";
    let observed = assert_binding_ends(
        &format!(
            "{NODE}fun run(): Unit {{ val node = Node(); borrow val borrowed = node; {first}; {last} }}"
        ),
        &[
            end(first, "borrowed", "node"),
            end(last, "borrowed", "node"),
        ],
        &[],
    );
    for unit in &observed.units {
        let [binding] = unit.borrow_results().bindings() else {
            panic!("one parent binding")
        };
        let [parent_end] = unit.borrow_results().ends() else {
            panic!("one actual parent end")
        };
        assert_eq!(parent_end.binding(), binding.binding());
        let UnitDropPoint::AfterStatement(statement) = parent_end.point() else {
            panic!("parent last-use statement")
        };
        let span = observed
            .parsed
            .ast()
            .statements()
            .get(statement.statement())
            .unwrap()
            .span();
        assert_eq!(observed.sources.slice(span).unwrap(), last);
        for child in unit.receiver_call_return_ends() {
            assert_ne!(child.point(), parent_end.point());
        }
        let UnitLoanTarget::Place(origin) = binding.origin() else {
            panic!("source")
        };
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(origin.root()))
            .collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), parent_end.point());
    }
}

#[test]
fn completed_parent_binding_allows_later_source_ownership_transfer() {
    let call = "borrowed.touch(\"last\", 0)";
    let observed = assert_binding_ends(
        &format!(
            "{NODE}fun consume(own value: Node): Unit {{}}\nfun run(own node: Node): Unit {{ borrow val borrowed = node; {call}; consume(node) }}"
        ),
        &[end(call, "borrowed", "node")],
        &[],
    );
    for unit in &observed.units {
        let [binding] = unit.borrow_results().bindings() else {
            panic!("binding")
        };
        let [parent_end] = unit.borrow_results().ends() else {
            panic!("last-use end")
        };
        let UnitDropPoint::AfterStatement(statement) = parent_end.point() else {
            panic!("last use")
        };
        assert_eq!(
            observed
                .sources
                .slice(
                    observed
                        .parsed
                        .ast()
                        .statements()
                        .get(statement.statement())
                        .unwrap()
                        .span()
                )
                .unwrap(),
            call
        );
        let UnitLoanTarget::Place(origin) = binding.origin() else {
            panic!("source")
        };
        assert!(
            unit.drops()
                .iter()
                .all(|drop| drop.target() != UnitDropTarget::Named(origin.root())),
            "caller moved its source rather than dropping it"
        );
    }
}

#[test]
fn nested_direct_binding_calls_keep_distinct_tokens_and_reverse_prefix_order() {
    for next in ["0", "return", "break", "continue"] {
        let inner = format!("borrowed.touch(\"inner\", {next})");
        let outer = format!("borrowed.touch(\"outer\", {inner})");
        let (open, close) = match next {
            "break" => ("loop {", "}"),
            "continue" => ("while (flag) {", "}"),
            _ => ("", ""),
        };
        let text = format!(
            "{NODE}fun run(node: Node, flag: Boolean): Unit {{ borrow val borrowed = node; {open} {outer} {close} }}"
        );
        let observed = if next == "0" {
            assert_binding_ends(
                &text,
                &[
                    end(&inner, "borrowed", "node"),
                    end(&outer, "borrowed", "node"),
                ],
                &[],
            )
        } else {
            assert_binding_ends(
                &text,
                &[],
                &[
                    (end(&inner, "borrowed", "node"), next, 1, 0),
                    (end(&outer, "borrowed", "node"), next, 2, 1),
                ],
            )
        };
        for unit in &observed.units {
            let tokens: Vec<_> = unit
                .receiver_call_return_ends()
                .iter()
                .map(|e| e.receiver())
                .chain(
                    unit.receiver_call_prefix_ends()
                        .iter()
                        .map(|e| e.receiver()),
                )
                .collect();
            assert_eq!(tokens[0].target(), tokens[1].target());
            assert_ne!(tokens[0].call(), tokens[1].call());
        }
    }
}

#[test]
fn loop_jump_keeps_the_inherited_parent_binding_for_a_later_use() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        let prefix = format!("borrowed.touch(\"prefix\", {jump})");
        let last = "borrowed.touch(\"after\", 0)";
        let observed = assert_binding_ends(
            &format!(
                "{NODE}fun run(own node: Node, flag: Boolean): Unit {{ borrow val borrowed = node; {loop_start} {{ {prefix} }}; {last} }}"
            ),
            &[end(last, "borrowed", "node")],
            &[(end(&prefix, "borrowed", "node"), jump, 1, 0)],
        );
        for unit in &observed.units {
            let prefix = unit.receiver_call_prefix_ends()[0].point();
            assert!(
                unit.borrow_results()
                    .ends()
                    .iter()
                    .all(|e| e.point() != prefix)
            );
            let UnitLoanTarget::Place(origin) = unit.borrow_results().bindings()[0].origin() else {
                panic!("source")
            };
            assert!(
                unit.drops().iter().all(|drop| {
                    drop.target() != UnitDropTarget::Named(origin.root()) || drop.point() != prefix
                }),
                "loop exit keeps the source owner, not necessarily argument temporaries"
            );
            assert_eq!(unit.borrow_results().ends().len(), 1);
        }
    }
}

#[test]
fn inner_loop_jump_does_not_retire_the_outer_binding_receiver() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        let inner = format!("borrowed.touch(\"inner\", {jump})");
        let outer = format!(
            "borrowed.touch(\"outer\", if (flag) {{ {loop_start} {{ {inner} }}; 0 }} else {{ 0 }})"
        );
        assert_binding_ends(
            &format!(
                "{NODE}fun run(node: Node, flag: Boolean): Unit {{ borrow val borrowed = node; {outer} }}"
            ),
            &[end(&outer, "borrowed", "node")],
            &[(end(&inner, "borrowed", "node"), jump, 1, 0)],
        );
    }
}

#[test]
fn completed_inner_binding_receiver_is_not_reissued_on_outer_exit() {
    for (open, close, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let outer = format!("borrowed.touch(borrowed.ready(), {jump})");
        assert_binding_ends(
            &format!(
                "class Node {{ fun ready(): String = \"ready\"; fun touch(text: String, own next: Int): Int = next }}\nfun run(node: Node, flag: Boolean): Unit {{ borrow val borrowed = node; {open} {outer} {close} }}"
            ),
            &[end("borrowed.ready()", "borrowed", "node")],
            &[(end(&outer, "borrowed", "node"), jump, 1, 0)],
        );
    }
}

#[test]
fn live_parent_binding_still_blocks_source_mutation_after_a_child_end() {
    let observed = observe(
        "class Node { fun read(): Int = 0; inout fun change(): Unit {} }\nfun run(inout node: Node): Unit { borrow val borrowed = node; borrowed.read(); node.change(); borrowed.read() }",
    );
    for unit in &observed.units {
        assert_eq!(unit.diagnostics().len(), 1);
        assert_eq!(unit.diagnostics()[0].code().to_string(), "L0135");
        assert!(unit.receiver_call_return_ends().is_empty());
        assert!(unit.receiver_call_prefix_ends().is_empty());
    }
}

#[test]
fn binding_receiver_in_for_keeps_actual_cleanup_and_parent_scope() {
    use lang_frontend::ownership_checking::UnitIterationCleanupAction as Action;
    for jump in ["return", "break", "continue"] {
        let call = format!("borrowed.touch(\"prefix\", {jump})");
        let observed = assert_binding_ends(
            &format!(
                "{NODE}fun run(node: Node): Unit {{ borrow val borrowed = node; for (item in listOf(1)) {{ {call} }} }}"
            ),
            &[],
            &[(end(&call, "borrowed", "node"), jump, 1, 0)],
        );
        for unit in &observed.units {
            let event = &unit.receiver_call_prefix_ends()[0];
            let actions = unit.iteration_cleanup_at(event.point()).unwrap();
            assert!(
                matches!(&actions[0], Action::EndCallLoan(loan) if loan.call() == event.receiver().call())
            );
            assert!(
                matches!(&actions[1], Action::EndReceiverLoan(receiver) if receiver == event.receiver())
            );
            assert_eq!(
                actions
                    .iter()
                    .any(|a| matches!(a, Action::FinishProvider { .. })),
                jump != "continue"
            );
        }
    }
}

#[test]
fn call_derived_parent_chain_and_alias_projections_stay_excluded() {
    for next in ["0", "return"] {
        for text in [
            format!(
                "{NODE}fun alias(source: Node): borrow Node from source = source\nfun run(node: Node): Unit {{ borrow val borrowed = alias(node); borrowed.touch(\"call\", {next}) }}"
            ),
            format!(
                "{NODE}fun run(node: Node): Unit {{ borrow val parent = node; borrow val borrowed = parent; borrowed.touch(\"chain\", {next}) }}"
            ),
            format!(
                "{NODE}class Holder(val node: Node)\nfun run(holder: Holder): Unit {{ borrow val borrowed = holder; borrowed.node.touch(\"field-receiver\", {next}) }}"
            ),
        ] {
            let observed = assert_binding_ends(&text, &[], &[]);
            for unit in &observed.units {
                assert!(!unit.receiver_facts().is_empty());
                assert!(!unit.borrow_results().bindings().is_empty());
            }
        }
    }
}

#[test]
fn borrowed_result_method_on_direct_binding_does_not_gain_an_end_observation() {
    let observed = assert_binding_ends(
        "class Node { fun view(source: Node): borrow Node from source = source }\nfun run(node: Node, source: Node): Unit { borrow val borrowed = node; borrow val result = borrowed.view(source) }",
        &[],
        &[],
    );
    for unit in &observed.units {
        assert_eq!(unit.receiver_facts().len(), 1);
        assert_eq!(unit.borrow_results().bindings().len(), 2);
    }
}

#[test]
fn conditional_binding_call_keeps_its_two_static_end_points_separate() {
    let call = "borrowed.touch(\"branch\", if (flag) { 0 } else { return })";
    let observed = assert_binding_ends(
        &format!(
            "{NODE}fun run(node: Node, flag: Boolean): Unit {{ borrow val borrowed = node; {call} }}"
        ),
        &[end(call, "borrowed", "node")],
        &[(end(call, "borrowed", "node"), "return", 1, 0)],
    );
    for unit in &observed.units {
        assert_eq!(
            unit.receiver_call_return_ends()[0].receiver(),
            unit.receiver_call_prefix_ends()[0].receiver()
        );
        assert_ne!(
            unit.receiver_call_return_ends()[0].point(),
            unit.receiver_call_prefix_ends()[0].point()
        );
    }
}

#[test]
fn direct_binding_does_not_gain_exclusive_receiver_access() {
    for next in ["0", "return"] {
        let observed = observe(&format!(
            "class Node {{ inout fun change(own next: Int): Int = next }}\nfun run(inout node: Node): Unit {{ borrow val borrowed = node; borrowed.change({next}) }}"
        ));
        for unit in &observed.units {
            let codes: Vec<_> = unit
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect();
            assert_eq!(codes, ["L0135"]);
            let diagnostic = &unit.diagnostics()[0];
            assert_eq!(
                diagnostic.message(),
                "new loan conflicts with an active loan"
            );
            assert_eq!(
                observed.sources.slice(diagnostic.primary_span()).unwrap(),
                "borrowed"
            );
            assert!(diagnostic.details().iter().any(|detail| {
                let lang_frontend::diagnostic::DiagnosticDetail::Label(label) = detail else {
                    return false;
                };
                label.message() == "conflicting loan starts here"
                    && observed.sources.slice(label.span()).unwrap() == "borrow"
            }));
            assert!(unit.receiver_call_return_ends().is_empty());
            assert!(unit.receiver_call_prefix_ends().is_empty());
        }
    }
}

#[test]
fn direct_binding_of_a_provider_element_keeps_the_loop_continuation_boundary() {
    for next in ["0", "return", "break", "continue"] {
        let observed = observe(&format!(
            "{NODE}fun run(nodes: List<Node>): Unit {{ for (node in nodes) {{ borrow val borrowed = node; borrowed.touch(\"provider\", {next}) }} }}"
        ));
        for unit in &observed.units {
            let codes: Vec<_> = unit
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect();
            assert_eq!(codes, ["L0164"]);
            let diagnostic = &unit.diagnostics()[0];
            assert_eq!(
                diagnostic.message(),
                "borrow result loop continuation is not yet proven"
            );
            assert_eq!(
                observed.sources.slice(diagnostic.primary_span()).unwrap(),
                "borrow"
            );
            assert!(unit.borrow_results().bindings().is_empty());
            assert!(unit.receiver_call_return_ends().is_empty());
            assert!(unit.receiver_call_prefix_ends().is_empty());
        }
    }
}
