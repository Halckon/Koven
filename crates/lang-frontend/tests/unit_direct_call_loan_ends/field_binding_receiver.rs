//! Direct field-initialized borrow bindings preserve the complete canonical source lease.
use super::receiver_return::NODE;
use super::*;
use lang_frontend::{
    ownership_checking::{
        BorrowBindingStorage, UnitReceiverOwnershipFact, UnitReceiverOwnershipKind,
        UnitReceiverOwnershipTarget,
    },
    type_checking::UnitCallReceiverOrigin,
};

struct FieldCall<'a> {
    call: &'a str,
    operand: &'a str,
    path: &'a [&'a str],
    initializer: &'a str,
}
fn field<'a>(
    call: &'a str,
    operand: &'a str,
    path: &'a [&'a str],
    initializer: &'a str,
) -> FieldCall<'a> {
    FieldCall {
        call,
        operand,
        path,
        initializer,
    }
}

fn assert_field(
    observed: &Observed,
    entry: usize,
    receiver: &UnitReceiverOwnershipFact,
    expected: &FieldCall<'_>,
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
    assert_eq!(receiver.call().source_unit(), source.id());
    let node = |id: UnitExpressionId| {
        assert_eq!(id.source_unit(), source.id());
        observed
            .parsed
            .ast()
            .expressions()
            .get(id.expression())
            .unwrap()
    };
    assert_eq!(
        observed
            .sources
            .slice(node(receiver.call()).span())
            .unwrap(),
        expected.call
    );
    assert_eq!(receiver.end_span(), node(receiver.call()).span());
    assert_eq!(receiver.kind(), UnitReceiverOwnershipKind::SharedLoan);
    assert!(!receiver.is_receiver_reservation());
    assert!(unit.receiver_facts().contains(receiver));
    let UnitCallReceiverOrigin::Expression(operand) = receiver.source() else {
        panic!("explicit field receiver")
    };
    assert_eq!(
        observed.sources.slice(node(operand).span()).unwrap(),
        expected.operand
    );
    assert_eq!(receiver.begin_span(), node(operand).span());
    let UnitReceiverOwnershipTarget::Place(place) = receiver.target() else {
        panic!("named field path")
    };
    assert!(!place.is_root());
    assert!(place.elements().is_empty());
    let path: Vec<_> = std::iter::once(place.root())
        .chain(place.fields().iter().copied())
        .map(|symbol| {
            assert_eq!(symbol.source_unit(), source.id());
            names.names().source_units()[symbol.source_unit().index()]
                .resolution()
                .symbols()[symbol.symbol().index()]
            .name()
        })
        .collect();
    assert_eq!(path, expected.path);
    let binding = unit
        .borrow_results()
        .bindings()
        .iter()
        .find(|binding| {
            names.names().source_units()[binding.binding().source_unit().index()]
                .resolution()
                .symbols()[binding.binding().symbol().index()]
            .name()
                == expected.operand.trim_matches(['(', ')'])
        })
        .unwrap();
    assert_eq!(binding.binding().source_unit(), source.id());
    assert_ne!(binding.binding(), place.root());
    assert_eq!(binding.origin(), &UnitLoanTarget::Place(place.clone()));
    assert_eq!(binding.storage(), BorrowBindingStorage::BorrowedStorage);
    assert!(binding.parent().is_none());
    assert!(binding.source_loan().is_none());
    assert_eq!(
        observed
            .sources
            .slice(node(binding.initializer()).span())
            .unwrap(),
        expected.initializer
    );
    assert!(
        unit.drops()
            .iter()
            .all(|drop| drop.target() != UnitDropTarget::Named(binding.binding())),
        "the alias never owns the payload"
    );
}

type Prefix<'a> = (FieldCall<'a>, &'a str, usize, usize);
fn assert_field_ends(text: &str, normal: &[FieldCall<'_>], prefixes: &[Prefix<'_>]) -> Observed {
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
            assert_field(&observed, entry, event.receiver(), expected);
            assert_eq!(
                event.point(),
                UnitDropPoint::CallReturn(event.receiver().call())
            );
        }
        for (event, (expected, jump, calls, receivers)) in
            unit.receiver_call_prefix_ends().iter().zip(prefixes)
        {
            assert_field(&observed, entry, event.receiver(), expected);
            let UnitDropPoint::ControlTransfer(transfer) = event.point() else {
                panic!("actual control transfer")
            };
            assert_eq!(
                transfer.source_unit(),
                event.receiver().call().source_unit()
            );
            let transfer = observed
                .parsed
                .ast()
                .expressions()
                .get(transfer.expression())
                .unwrap();
            assert_eq!(observed.sources.slice(transfer.span()).unwrap(), *jump);
            assert_eq!(event.preceding_drops(), 0);
            assert_eq!(event.preceding_call_loan_ends(), *calls);
            assert_eq!(event.preceding_receiver_loan_ends(), *receivers);
        }
    }
    observed
}

#[test]
fn original_field_binding_normal_source_observes_its_child_end() {
    assert_field_ends(
        &format!(
            "{NODE}class Holder(val node: Node)\nfun run(holder: Holder): Unit {{ borrow val borrowed = holder.node; borrowed.touch(\"field-source\", 0) }}"
        ),
        &[field(
            "borrowed.touch(\"field-source\", 0)",
            "borrowed",
            &["holder", "node"],
            "holder.node",
        )],
        &[],
    );
}

#[test]
fn original_field_binding_return_source_observes_its_child_end() {
    assert_field_ends(
        &format!(
            "{NODE}class Holder(val node: Node)\nfun run(holder: Holder): Unit {{ borrow val borrowed = holder.node; borrowed.touch(\"field-source\", return) }}"
        ),
        &[],
        &[(
            field(
                "borrowed.touch(\"field-source\", return)",
                "borrowed",
                &["holder", "node"],
                "holder.node",
            ),
            "return",
            1,
            0,
        )],
    );
}

#[test]
fn grouped_multifield_initializer_keeps_the_complete_source_for_all_root_modes() {
    for mode in ["own ", "", "borrow ", "inout "] {
        for (open, close, next) in [
            ("", "", "0"),
            ("", "", "return"),
            ("loop {", "}", "break"),
            ("while (flag) {", "}", "continue"),
        ] {
            let call = format!("((borrowed)).touch(\"group\", {next})");
            let init = "((holder.inner).node)";
            let text = format!(
                "{NODE}class Inner(val node: Node)\nclass Holder(val inner: Inner)\nfun run({mode}holder: Holder, flag: Boolean): Unit {{ borrow val borrowed = {init}; {open} {call} {close} }}"
            );
            let expected = field(&call, "((borrowed))", &["holder", "inner", "node"], init);
            let observed = if next == "0" {
                assert_field_ends(&text, &[expected], &[])
            } else {
                assert_field_ends(&text, &[], &[(expected, next, 1, 0)])
            };
            if mode != "own " {
                for unit in &observed.units {
                    let UnitLoanTarget::Place(origin) =
                        unit.borrow_results().bindings()[0].origin()
                    else {
                        panic!("source")
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
fn distinct_field_binding_tokens_keep_sibling_paths_and_reverse_prefix_order() {
    for next in ["0", "return", "break", "continue"] {
        let inner = format!("right.touch(\"inner\", {next})");
        let outer = format!("left.touch(\"outer\", {inner})");
        let (open, close) = match next {
            "break" => ("loop {", "}"),
            "continue" => ("while (flag) {", "}"),
            _ => ("", ""),
        };
        let text = format!(
            "{NODE}class Holder(val left: Node, val right: Node)\nfun run(holder: Holder, flag: Boolean): Unit {{ borrow val left = holder.left; borrow val right = holder.right; {open} {outer} {close} }}"
        );
        let first = field(&inner, "right", &["holder", "right"], "holder.right");
        let second = field(&outer, "left", &["holder", "left"], "holder.left");
        let observed = if next == "0" {
            assert_field_ends(&text, &[first, second], &[])
        } else {
            assert_field_ends(&text, &[], &[(first, next, 1, 0), (second, next, 2, 1)])
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
            assert_ne!(tokens[0].target(), tokens[1].target());
            assert_ne!(tokens[0].call(), tokens[1].call());
        }
    }
}

#[test]
fn completed_field_child_keeps_parent_binding_until_its_last_use_and_one_aggregate_drop() {
    let first = "borrowed.touch(\"first\", 0)";
    let last = "borrowed.touch(\"last\", 0)";
    let observed = assert_field_ends(
        &format!(
            "{NODE}class Holder(val node: Node)\nfun run(): Unit {{ val holder = Holder(Node()); borrow val borrowed = holder.node; {first}; {last} }}"
        ),
        &[
            field(first, "borrowed", &["holder", "node"], "holder.node"),
            field(last, "borrowed", &["holder", "node"], "holder.node"),
        ],
        &[],
    );
    for unit in &observed.units {
        let [parent_end] = unit.borrow_results().ends() else {
            panic!("one parent end")
        };
        let UnitDropPoint::AfterStatement(statement) = parent_end.point() else {
            panic!("last-use end")
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
            last
        );
        for child in unit.receiver_call_return_ends() {
            assert_ne!(child.point(), parent_end.point());
        }
        let UnitLoanTarget::Place(origin) = unit.borrow_results().bindings()[0].origin() else {
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
fn completed_field_parent_allows_moving_the_aggregate_later() {
    let call = "borrowed.touch(\"last\", 0)";
    let observed = assert_field_ends(
        &format!(
            "{NODE}class Holder(val node: Node)\nfun consume(own holder: Holder): Unit {{}}\nfun run(own holder: Holder): Unit {{ borrow val borrowed = holder.node; {call}; consume(holder) }}"
        ),
        &[field(call, "borrowed", &["holder", "node"], "holder.node")],
        &[],
    );
    for unit in &observed.units {
        let [parent_end] = unit.borrow_results().ends() else {
            panic!("one parent end")
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
        let UnitLoanTarget::Place(origin) = unit.borrow_results().bindings()[0].origin() else {
            panic!("source")
        };
        assert!(
            unit.drops()
                .iter()
                .all(|drop| drop.target() != UnitDropTarget::Named(origin.root())),
            "caller moved the aggregate"
        );
    }
}

#[test]
fn live_field_binding_allows_sibling_mutation_but_blocks_overlapping_replacement() {
    let call = "borrowed.touch(\"live\", 0)";
    let types = format!("{NODE}class Holder(var left: Node, var right: Node)\n");
    assert_field_ends(
        &format!(
            "{types}fun run(inout holder: Holder): Unit {{ borrow val borrowed = holder.left; holder.right = Node(); {call} }}"
        ),
        &[field(call, "borrowed", &["holder", "left"], "holder.left")],
        &[],
    );
    let observed = observe(&format!(
        "{types}fun run(inout holder: Holder): Unit {{ borrow val borrowed = holder.left; holder.left = Node(); {call} }}"
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
            "mutation conflicts with an active loan"
        );
        assert_eq!(
            observed.sources.slice(diagnostic.primary_span()).unwrap(),
            "holder.left"
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

#[test]
fn live_field_parent_still_blocks_whole_aggregate_move_after_child_completion() {
    let observed = observe(&format!(
        "{NODE}class Holder(val node: Node)\nfun consume(own holder: Holder): Unit {{}}\nfun run(own holder: Holder): Unit {{ borrow val borrowed = holder.node; borrowed.touch(\"first\", 0); consume(holder); borrowed.touch(\"last\", 0) }}"
    ));
    for unit in &observed.units {
        let codes: Vec<_> = unit
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        assert_eq!(codes, ["L0135"]);
        let diagnostic = &unit.diagnostics()[0];
        assert_eq!(diagnostic.message(), "move conflicts with an active loan");
        assert_eq!(
            observed.sources.slice(diagnostic.primary_span()).unwrap(),
            "holder"
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

#[test]
fn field_binding_loop_jump_preserves_the_parent_for_a_later_use() {
    for (open, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        let prefix = format!("borrowed.touch(\"prefix\", {jump})");
        let last = "borrowed.touch(\"after\", 0)";
        let observed = assert_field_ends(
            &format!(
                "{NODE}class Holder(val node: Node)\nfun run(own holder: Holder, flag: Boolean): Unit {{ borrow val borrowed = holder.node; {open} {{ {prefix} }}; {last} }}"
            ),
            &[field(last, "borrowed", &["holder", "node"], "holder.node")],
            &[(
                field(&prefix, "borrowed", &["holder", "node"], "holder.node"),
                jump,
                1,
                0,
            )],
        );
        for unit in &observed.units {
            let point = unit.receiver_call_prefix_ends()[0].point();
            assert_eq!(unit.borrow_results().ends().len(), 1);
            assert!(
                unit.borrow_results()
                    .ends()
                    .iter()
                    .all(|e| e.point() != point)
            );
            let UnitLoanTarget::Place(origin) = unit.borrow_results().bindings()[0].origin() else {
                panic!("source")
            };
            assert!(
                unit.drops()
                    .iter()
                    .all(|drop| drop.target() != UnitDropTarget::Named(origin.root())
                        || drop.point() != point)
            );
        }
    }
}

#[test]
fn inner_loop_jump_keeps_outer_field_binding_receiver_pending() {
    for (open, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        let inner = format!("borrowed.touch(\"inner\", {jump})");
        let outer = format!(
            "borrowed.touch(\"outer\", if (flag) {{ {open} {{ {inner} }}; 0 }} else {{ 0 }})"
        );
        assert_field_ends(
            &format!(
                "{NODE}class Holder(val node: Node)\nfun run(holder: Holder, flag: Boolean): Unit {{ borrow val borrowed = holder.node; {outer} }}"
            ),
            &[field(
                &outer,
                "borrowed",
                &["holder", "node"],
                "holder.node",
            )],
            &[(
                field(&inner, "borrowed", &["holder", "node"], "holder.node"),
                jump,
                1,
                0,
            )],
        );
    }
}

#[test]
fn completed_inner_field_binding_receiver_is_not_reissued_on_outer_exit() {
    for (open, close, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let outer = format!("borrowed.touch(borrowed.ready(), {jump})");
        assert_field_ends(
            &format!(
                "class Node {{ fun ready(): String = \"ready\"; fun touch(text: String, own next: Int): Int = next }}\nclass Holder(val node: Node)\nfun run(holder: Holder, flag: Boolean): Unit {{ borrow val borrowed = holder.node; {open} {outer} {close} }}"
            ),
            &[field(
                "borrowed.ready()",
                "borrowed",
                &["holder", "node"],
                "holder.node",
            )],
            &[(
                field(&outer, "borrowed", &["holder", "node"], "holder.node"),
                jump,
                1,
                0,
            )],
        );
    }
}

#[test]
fn field_binding_in_for_observes_the_actual_prefix_cleanup() {
    use lang_frontend::ownership_checking::UnitIterationCleanupAction as Action;
    for jump in ["return", "break", "continue"] {
        let call = format!("borrowed.touch(\"prefix\", {jump})");
        let observed = assert_field_ends(
            &format!(
                "{NODE}class Holder(val node: Node)\nfun run(holder: Holder): Unit {{ borrow val borrowed = holder.node; for (item in listOf(1)) {{ {call} }} }}"
            ),
            &[],
            &[(
                field(&call, "borrowed", &["holder", "node"], "holder.node"),
                jump,
                1,
                0,
            )],
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
fn conditional_field_binding_call_preserves_mutually_exclusive_end_points() {
    let call = "borrowed.touch(\"branch\", if (flag) { 0 } else { return })";
    let observed = assert_field_ends(
        &format!(
            "{NODE}class Holder(val node: Node)\nfun run(holder: Holder, flag: Boolean): Unit {{ borrow val borrowed = holder.node; {call} }}"
        ),
        &[field(call, "borrowed", &["holder", "node"], "holder.node")],
        &[(
            field(call, "borrowed", &["holder", "node"], "holder.node"),
            "return",
            1,
            0,
        )],
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
fn field_source_can_be_replaced_only_after_the_parent_last_use() {
    let call = "borrowed.touch(\"last\", 0)";
    let replacement = "holder.node = Node()";
    let observed = assert_field_ends(
        &format!(
            "{NODE}class Holder(var node: Node)\nfun run(inout holder: Holder): Unit {{ borrow val borrowed = holder.node; {call}; {replacement} }}"
        ),
        &[field(call, "borrowed", &["holder", "node"], "holder.node")],
        &[],
    );
    for unit in &observed.units {
        let [parent_end] = unit.borrow_results().ends() else {
            panic!("parent last use")
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
        let UnitLoanTarget::Place(origin) = unit.borrow_results().bindings()[0].origin() else {
            panic!("source")
        };
        let replacements: Vec<_> = unit
            .drops()
            .iter()
            .filter_map(|drop| {
                let UnitDropTarget::ReplacedField { assignment, field } = drop.target() else {
                    return None;
                };
                Some((drop, assignment, field))
            })
            .collect();
        let [(drop, assignment, field)] = replacements.as_slice() else {
            panic!("one old field drop")
        };
        assert_eq!(*field, origin.fields()[0]);
        assert_eq!(drop.point(), UnitDropPoint::BeforeReplacement(*assignment));
        assert_eq!(
            observed
                .sources
                .slice(
                    observed
                        .parsed
                        .ast()
                        .expressions()
                        .get(assignment.expression())
                        .unwrap()
                        .span()
                )
                .unwrap(),
            replacement
        );
        assert_ne!(drop.point(), parent_end.point());
    }
}

#[test]
fn direct_field_binding_of_a_value_class_keeps_the_checked_field_identity() {
    for next in ["0", "return"] {
        let call = format!("borrowed.touch(\"value\", {next})");
        let text = format!(
            "value class Node(val value: Int) {{ fun touch(text: String, own next: Int): Int = value + next }}\nclass Holder(val node: Node)\nfun run(holder: Holder): Unit {{ borrow val borrowed = holder.node; {call} }}"
        );
        let expected = field(&call, "borrowed", &["holder", "node"], "holder.node");
        if next == "0" {
            assert_field_ends(&text, &[expected], &[]);
        } else {
            assert_field_ends(&text, &[], &[(expected, next, 1, 0)]);
        }
    }
}

#[test]
fn call_derived_field_parent_chain_alias_projection_and_this_keep_their_boundaries() {
    for next in ["0", "return"] {
        for text in [
            format!(
                "{NODE}class Holder(val node: Node)\nfun view(holder: Holder): borrow Node from holder = holder.node\nfun run(holder: Holder): Unit {{ borrow val borrowed = view(holder); borrowed.touch(\"call\", {next}) }}"
            ),
            format!(
                "{NODE}class Holder(val node: Node)\nfun run(holder: Holder): Unit {{ borrow val parent = holder.node; borrow val borrowed = parent; borrowed.touch(\"chain\", {next}) }}"
            ),
            format!(
                "{NODE}class Holder(val node: Node)\nfun run(holder: Holder): Unit {{ borrow val borrowed = holder; borrowed.node.touch(\"projection\", {next}) }}"
            ),
            format!(
                "{NODE}class Holder(val node: Node) {{ fun run(): Unit {{ borrow val borrowed = this.node; borrowed.touch(\"this-field\", {next}) }} }}"
            ),
        ] {
            let observed = assert_field_ends(&text, &[], &[]);
            for unit in &observed.units {
                assert!(!unit.borrow_results().bindings().is_empty());
                assert!(!unit.receiver_facts().is_empty());
            }
        }
    }
}

#[test]
fn field_binding_exclusive_receiver_is_rejected_by_the_actual_parent_loan() {
    use lang_frontend::diagnostic::DiagnosticDetail;
    for next in ["0", "return"] {
        let observed = observe(&format!(
            "class Node {{ inout fun change(own next: Int): Int = next }}\nclass Holder(val node: Node)\nfun run(holder: Holder): Unit {{ borrow val borrowed = holder.node; borrowed.change({next}) }}"
        ));
        for unit in &observed.units {
            let [diagnostic] = unit.diagnostics() else {
                panic!("one conflict")
            };
            assert_eq!(diagnostic.code().to_string(), "L0135");
            assert_eq!(
                diagnostic.message(),
                "new loan conflicts with an active loan"
            );
            assert_eq!(
                observed.sources.slice(diagnostic.primary_span()).unwrap(),
                "borrowed"
            );
            assert!(diagnostic.details().iter().any(|detail| matches!(detail, DiagnosticDetail::Label(label) if label.message() == "conflicting loan starts here" && observed.sources.slice(label.span()).unwrap() == "borrow")));
            assert!(unit.receiver_call_return_ends().is_empty());
            assert!(unit.receiver_call_prefix_ends().is_empty());
        }
    }
}

#[test]
fn field_binding_borrowed_result_method_does_not_gain_an_end_observation() {
    let observed = assert_field_ends(
        "class Node { fun view(source: Node): borrow Node from source = source }\nclass Holder(val node: Node)\nfun run(holder: Holder, source: Node): Unit { borrow val borrowed = holder.node; borrow val result = borrowed.view(source) }",
        &[],
        &[],
    );
    for unit in &observed.units {
        assert_eq!(unit.receiver_facts().len(), 1);
        assert_eq!(unit.borrow_results().bindings().len(), 2);
    }
}

#[test]
fn moveonly_index_initializer_is_rejected_before_field_binding_end_observation() {
    use lang_frontend::diagnostic::DiagnosticDetail;
    for next in ["0", "return"] {
        let observed = observe(&format!(
            "{NODE}fun run(nodes: List<Node>): Unit {{ borrow val borrowed = nodes[0]; borrowed.touch(\"index\", {next}) }}"
        ));
        for unit in &observed.units {
            let [diagnostic] = unit.diagnostics() else {
                panic!("one rejected index")
            };
            assert_eq!(diagnostic.code().to_string(), "L0136");
            assert_eq!(
                diagnostic.message(),
                "cannot move a non-Copyable element out of a sequential container"
            );
            assert_eq!(
                observed.sources.slice(diagnostic.primary_span()).unwrap(),
                "nodes[0]"
            );
            assert!(diagnostic.details().iter().any(|detail| matches!(detail, DiagnosticDetail::Label(label) if observed.sources.slice(label.span()).unwrap() == "nodes")));
            assert!(unit.borrow_results().bindings().is_empty());
            assert!(unit.receiver_call_return_ends().is_empty());
            assert!(unit.receiver_call_prefix_ends().is_empty());
        }
    }
}
