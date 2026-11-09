//! Return-only Shared receiver observation; existing argument and Single slices stay intact.
use super::*;
use lang_frontend::{
    ownership_checking::{UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget},
    type_checking::UnitCallReceiverOrigin,
};

const NODE: &str = "class Node { fun touch(text: String, own next: Int): Int = next }\n";

fn assert_prefixes(text: &str, expected: &[(&str, &str, usize, usize)]) -> Observed {
    let observed = observe(text);
    assert_units_admitted(&observed);
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        let [source] = names.names().index().source_units() else {
            panic!("one source")
        };
        let [local] = names.names().source_units() else {
            panic!("one resolution")
        };
        let node = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), source.id());
            observed
                .parsed
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap()
        };
        let events = unit.receiver_call_prefix_ends();
        assert_eq!(events.len(), expected.len(), "{text}");
        for (event, &(call_text, root_text, call_rank, receiver_rank)) in
            events.iter().zip(expected)
        {
            let receiver = event.receiver();
            assert!(unit.receiver_facts().contains(receiver));
            assert_eq!(receiver.kind(), UnitReceiverOwnershipKind::SharedLoan);
            assert!(!receiver.is_receiver_reservation());
            assert_eq!(
                observed.sources.slice(node(receiver.call()).span()),
                Ok(call_text)
            );
            assert_eq!(receiver.end_span(), node(receiver.call()).span());
            let UnitCallReceiverOrigin::Expression(operand) = receiver.source() else {
                panic!("explicit receiver")
            };
            assert_eq!(observed.sources.slice(node(operand).span()), Ok(root_text));
            let UnitReceiverOwnershipTarget::Place(place) = receiver.target() else {
                panic!("owned root")
            };
            assert!(place.is_root());
            assert_eq!(place.root().source_unit(), source.id());
            let symbol = &local.resolution().symbols()[place.root().symbol().index()];
            assert_eq!(symbol.name(), root_text.trim_matches(['(', ')']));
            let UnitDropPoint::ControlTransfer(transfer) = event.point() else {
                panic!("Return endpoint")
            };
            assert!(matches!(
                node(transfer).payload(),
                Expression::Return { .. }
            ));
            assert_eq!(observed.sources.slice(node(transfer).span()), Ok("return"));
            assert_eq!(event.preceding_drops(), 0);
            assert_eq!(event.preceding_call_loan_ends(), call_rank);
            assert_eq!(event.preceding_receiver_loan_ends(), receiver_rank);
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
                .collect();
            assert_eq!(drops.len(), 1, "original receiver owner drops once");
            assert_eq!(drops[0].point(), event.point());
        }
    }
    observed
}

#[test]
fn owned_root_receiver_ends_after_its_argument_before_owner_drop() {
    for root in ["node", "((node))"] {
        let call = format!("{root}.touch(\"prefix\", return)");
        let observed = assert_prefixes(
            &format!("{NODE}fun run(own node: Node): Unit {{ {call} }}"),
            &[(&call, root, 1, 0)],
        );
        for unit in &observed.units {
            assert!(
                unit.direct_call_loan_ends().is_empty(),
                "argument slice unchanged"
            );
            assert_eq!(unit.loans().len(), 1);
        }
    }
}

#[test]
fn nested_receivers_follow_actual_reverse_unwind_order() {
    assert_prefixes(
        &format!(
            "{NODE}fun run(): Unit {{ val outer = Node(); val inner = Node(); outer.touch(\"outer\", inner.touch(\"inner\", return)) }}"
        ),
        &[
            ("inner.touch(\"inner\", return)", "inner", 1, 0),
            (
                "outer.touch(\"outer\", inner.touch(\"inner\", return))",
                "outer",
                2,
                1,
            ),
        ],
    );
}

#[test]
fn distinct_receiver_tokens_for_the_same_owner_are_not_deduplicated() {
    assert_prefixes(
        &format!(
            "{NODE}fun run(): Unit {{ val node = Node(); node.touch(\"outer\", node.touch(\"inner\", return)) }}"
        ),
        &[
            ("node.touch(\"inner\", return)", "node", 1, 0),
            (
                "node.touch(\"outer\", node.touch(\"inner\", return))",
                "node",
                2,
                1,
            ),
        ],
    );
}

#[test]
fn normal_calls_unvisited_calls_and_loop_jumps_are_not_return_prefixes() {
    for body in [
        "node.touch(\"normal\", 0); return",
        "return; node.touch(\"unreachable\", 0)",
        "loop { node.touch(\"break\", break) }",
        "while (flag) { node.touch(\"continue\", continue) }",
        "node.touch(\"outer\", if (flag) { loop { break }; 0 } else { 0 })",
    ] {
        assert_prefixes(
            &format!("{NODE}fun run(own node: Node, flag: Boolean): Unit {{ {body} }}"),
            &[],
        );
    }
}

#[test]
fn borrowed_temporary_projected_this_and_reserved_receivers_remain_excluded() {
    for text in [
        format!("{NODE}fun run(node: Node): Unit {{ node.touch(\"borrowed\", return) }}"),
        format!("{NODE}fun run(): Unit {{ Node().touch(\"temporary\", return) }}"),
        format!("{NODE}class Holder(val node: Node)\nfun run(own holder: Holder): Unit {{ holder.node.touch(\"field\", return) }}"),
        format!("{NODE}fun run(own nodes: List<Node>): Unit {{ for (node in nodes) {{ node.touch(\"provider\", return) }} }}"),
        "class Node { fun touch(text: String, own next: Int): Int = next; fun run(): Unit { touch(\"implicit\", return) } }".into(),
        "class Node { fun touch(text: String, own next: Int): Int = next; fun run(): Unit { this.touch(\"this\", return) } }".into(),
        "class Node { inout fun touch(text: String, own next: Int): Int = next }\nfun run(): Unit { var node = Node(); node.touch(\"reservation\", return) }".into(),
    ] {
        let observed = assert_prefixes(&text, &[]);
        for unit in &observed.units {
            assert!(!unit.receiver_facts().is_empty(), "real receiver descriptors remain");
        }
    }
}

#[test]
fn later_error_or_deferred_source_revokes_a_nonempty_receiver_prefix() {
    let prefix =
        format!("{NODE}fun run(): Unit {{ val node = Node(); node.touch(\"prefix\", return) }}\n");
    assert_prefixes(&prefix, &[("node.touch(\"prefix\", return)", "node", 1, 0)]);
    for (suffix, deferred) in [
        ("fun rejected(): Unit { val target = 0; target = 1 }", false),
        (
            "fun rejected(): Unit { var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 }) }",
            true,
        ),
    ] {
        let observed = observe(&format!("{prefix}{suffix}"));
        for unit in &observed.units {
            if deferred {
                assert!(unit.diagnostics().is_empty());
                assert!(!unit.deferred().is_empty());
            } else {
                assert!(
                    unit.diagnostics()
                        .iter()
                        .any(|d| d.code().to_string() == "L0134")
                );
            }
            assert!(unit.receiver_call_prefix_ends().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}

#[test]
fn order_counts_a_real_borrowed_receiver_outside_the_published_slice() {
    assert_prefixes(
        &format!(
            "{NODE}fun run(own outer: Node, inner: Node): Unit {{ outer.touch(\"outer\", inner.touch(\"inner\", return)) }}"
        ),
        &[(
            "outer.touch(\"outer\", inner.touch(\"inner\", return))",
            "outer",
            2,
            1,
        )],
    );
}

#[test]
fn explicit_borrow_binding_receivers_remain_outside_the_slice() {
    let observed = assert_prefixes(
        &format!(
            "{NODE}fun alias(source: Node): borrow Node from source = source\nfun run(): Unit {{ val node = Node(); borrow val borrowed = alias(node); borrowed.touch(\"binding\", return) }}"
        ),
        &[],
    );
    for unit in &observed.units {
        assert!(!unit.receiver_facts().is_empty());
        assert_eq!(unit.borrow_results().bindings().len(), 1);
    }
}

#[test]
fn borrow_result_receiver_source_retains_its_existing_rejection() {
    use lang_frontend::diagnostic::DiagnosticDetail;
    let observed = observe(
        "class Node { fun view(own next: Int): borrow Node from this = this }\nfun run(own node: Node): Unit { node.view(return) }",
    );
    let source = observed.parsed.source_id();
    for unit in &observed.units {
        let diagnostics = unit.diagnostics();
        assert_eq!(diagnostics.len(), 2);
        for (diagnostic, (start, end, message)) in diagnostics.iter().zip([
            (
                38,
                44,
                "borrow result source requires an unsupported receiver or inout continuation",
            ),
            (
                101,
                118,
                "caller borrow result continuation is not yet proven",
            ),
        ]) {
            assert_eq!(diagnostic.code().to_string(), "L0164");
            assert_eq!(diagnostic.message(), message);
            let span = diagnostic.primary_span();
            assert_eq!(
                (span.source_id(), span.start(), span.end()),
                (source, start, end)
            );
        }
        assert!(diagnostics[0].details().is_empty());
        let [DiagnosticDetail::Label(label)] = diagnostics[1].details() else {
            panic!("original declaration label")
        };
        assert_eq!(
            (
                label.span().source_id(),
                label.span().start(),
                label.span().end()
            ),
            (source, 38, 44)
        );
        assert_eq!(label.message(), "borrow result declared here");
        assert!(unit.receiver_call_prefix_ends().is_empty());
        assert!(unit.receiver_facts().is_empty());
        assert!(unit.clone().validate().is_err());
    }
}

#[test]
fn distinct_return_points_restart_all_prefix_counters() {
    let observed = assert_prefixes(
        &format!(
            "{NODE}fun first(own left: Node): Unit {{ left.touch(\"left\", return) }}\nfun second(own right: Node): Unit {{ right.touch(\"right\", return) }}"
        ),
        &[
            ("left.touch(\"left\", return)", "left", 1, 0),
            ("right.touch(\"right\", return)", "right", 1, 0),
        ],
    );
    for unit in &observed.units {
        assert_ne!(
            unit.receiver_call_prefix_ends()[0].point(),
            unit.receiver_call_prefix_ends()[1].point()
        );
    }
}

// 按既有宿主边界只编译，正式 CI 才执行 Abort 整项。
#[test]
fn abort_does_not_publish_receiver_prefix_ends() {
    let observed = assert_prefixes(
        &format!(
            "{NODE}fun run(): Unit {{ val node = Node(); node.touch(\"prefix\", error(\"stop\")) }}"
        ),
        &[],
    );
    for unit in &observed.units {
        assert!(!unit.receiver_facts().is_empty());
        assert!(unit.drops().is_empty());
    }
}

#[test]
fn admitted_parameter_borrow_result_method_is_outside_the_owned_result_slice() {
    let observed = assert_prefixes(
        "class Node { fun view(source: Node, own next: Int): borrow Node from source = source }\nfun run(own node: Node, source: Node): Unit { borrow val unused = node.view(source, return) }",
        &[],
    );
    for (unit, typed) in observed.units.iter().zip(&observed.unit_types) {
        let [receiver] = unit.receiver_facts() else {
            panic!("one actual pending receiver")
        };
        assert_eq!(receiver.kind(), UnitReceiverOwnershipKind::SharedLoan);
        let call = typed.types().call(receiver.call()).unwrap();
        assert!(matches!(
            call.result_source(),
            lang_frontend::type_checking::CallableResultSource::Borrow(_)
        ));
        assert!(
            unit.borrow_results().bindings().is_empty(),
            "Return abandons the initializer"
        );
    }
}
