//! Observe actual receiver-prefix cleanup at loop jumps without inventing new unwind edges.
use super::*;
use lang_frontend::{
    ownership_checking::{UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget},
    type_checking::UnitCallReceiverOrigin,
};

const NODE: &str = "class Node { fun touch(text: String, own next: Int): Int = next }\n";

pub(super) struct Prefix<'a> {
    pub(super) call: &'a str,
    pub(super) operand: &'a str,
    pub(super) jump: &'a str,
    pub(super) calls_before: usize,
    pub(super) receivers_before: usize,
    pub(super) drop_at_jump: bool,
}

pub(super) fn assert_prefixes(text: &str, expected: &[Prefix<'_>]) -> Observed {
    let observed = observe(text);
    assert_units_admitted(&observed);
    for (entry, (unit, names)) in observed.units.iter().zip(&observed.unit_names).enumerate() {
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
        assert_eq!(
            events.len(),
            expected.len(),
            "complete bounded vector: {text}"
        );
        for (event, expected) in events.iter().zip(expected) {
            let receiver = event.receiver();
            assert!(unit.receiver_facts().contains(receiver));
            assert_eq!(receiver.kind(), UnitReceiverOwnershipKind::SharedLoan);
            assert!(!receiver.is_receiver_reservation());
            let call = node(receiver.call());
            assert_eq!(observed.sources.slice(call.span()), Ok(expected.call));
            // Locate the unique call independently from the source, not from another fact.
            let start = text.find(expected.call).unwrap();
            assert_eq!(
                (call.span().start(), call.span().end()),
                (start, start + expected.call.len())
            );
            assert_eq!(receiver.end_span(), call.span());
            let UnitCallReceiverOrigin::Expression(operand) = receiver.source() else {
                panic!("explicit receiver")
            };
            assert_eq!(
                observed.sources.slice(node(operand).span()),
                Ok(expected.operand)
            );
            let UnitReceiverOwnershipTarget::Place(place) = receiver.target() else {
                panic!("owned root")
            };
            assert!(place.is_root());
            assert_eq!(place.root().source_unit(), source.id());
            assert_eq!(
                local.resolution().symbols()[place.root().symbol().index()].name(),
                expected.operand.trim_matches(['(', ')'])
            );
            let UnitDropPoint::ControlTransfer(jump) = event.point() else {
                panic!("loop jump")
            };
            let jump = node(jump);
            match expected.jump {
                "break" => assert!(matches!(jump.payload(), Expression::Break { .. })),
                "continue" => assert!(matches!(jump.payload(), Expression::Continue { .. })),
                "return" => assert!(matches!(jump.payload(), Expression::Return { .. })),
                _ => panic!("fixed control transfer"),
            }
            assert_eq!(observed.sources.slice(jump.span()), Ok(expected.jump));
            let jump_start = start + expected.call.rfind(expected.jump).unwrap();
            assert_eq!(
                (jump.span().start(), jump.span().end()),
                (jump_start, jump_start + expected.jump.len())
            );
            assert_eq!(event.preceding_drops(), 0);
            assert_eq!(event.preceding_call_loan_ends(), expected.calls_before);
            assert_eq!(
                event.preceding_receiver_loan_ends(),
                expected.receivers_before
            );
            let drops_at_jump: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| {
                    drop.target() == UnitDropTarget::Named(place.root())
                        && drop.point() == event.point()
                })
                .collect();
            assert_eq!(drops_at_jump.len(), usize::from(expected.drop_at_jump));
        }
    }
    observed
}

#[test]
fn local_receiver_prefix_ends_on_break_and_continue_before_its_owner() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        for operand in ["node", "((node))"] {
            let call = format!("{operand}.touch(\"prefix\", {jump})");
            assert_prefixes(
                &format!(
                    "{NODE}fun run(flag: Boolean): Unit {{ {loop_start} {{ val node = Node(); {call} }} }}"
                ),
                &[Prefix {
                    call: &call,
                    operand,
                    jump,
                    calls_before: 1,
                    receivers_before: 0,
                    drop_at_jump: true,
                }],
            );
        }
    }
}

#[test]
fn nested_same_root_receiver_tokens_keep_reverse_order_at_each_loop_jump() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        let inner = format!("node.touch(\"inner\", {jump})");
        let outer = format!("node.touch(\"outer\", {inner})");
        assert_prefixes(
            &format!(
                "{NODE}fun run(flag: Boolean): Unit {{ {loop_start} {{ val node = Node(); {outer} }} }}"
            ),
            &[
                Prefix {
                    call: &inner,
                    operand: "node",
                    jump,
                    calls_before: 1,
                    receivers_before: 0,
                    drop_at_jump: true,
                },
                Prefix {
                    call: &outer,
                    operand: "node",
                    jump,
                    calls_before: 2,
                    receivers_before: 1,
                    drop_at_jump: true,
                },
            ],
        );
    }
}

#[test]
fn inner_loop_jump_does_not_end_the_outer_pending_receiver() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        let inner = format!("inner.touch(\"inner\", {jump})");
        let text = format!(
            "{NODE}fun run(flag: Boolean): Unit {{ val outer = Node(); outer.touch(\"outer\", if (flag) {{ {loop_start} {{ val inner = Node(); {inner} }}; 0 }} else {{ 0 }}) }}"
        );
        let observed = assert_prefixes(
            &text,
            &[Prefix {
                call: &inner,
                operand: "inner",
                jump,
                calls_before: 1,
                receivers_before: 0,
                drop_at_jump: true,
            }],
        );
        for unit in &observed.units {
            assert_eq!(
                unit.receiver_facts().len(),
                2,
                "outer receiver descriptor still exists"
            );
            let point = unit.receiver_call_prefix_ends()[0].point();
            assert_eq!(
                unit.drops()
                    .iter()
                    .filter(|drop| drop.point() == point
                        && matches!(drop.target(), UnitDropTarget::Named(_)))
                    .count(),
                1,
                "only inner owner is dropped on the nested edge"
            );
        }
    }
}

#[test]
fn continue_preserves_the_outer_owner_and_counts_excluded_borrowed_receivers() {
    let inner = "borrowed.touch(\"inner\", continue)";
    let outer = format!("owned.touch(\"outer\", {inner})");
    assert_prefixes(
        &format!(
            "{NODE}fun run(own owned: Node, borrowed: Node, flag: Boolean): Unit {{ while (flag) {{ {outer} }} }}"
        ),
        &[Prefix {
            call: &outer,
            operand: "owned",
            jump: "continue",
            calls_before: 2,
            receivers_before: 1,
            drop_at_jump: false,
        }],
    );
}

#[test]
fn receiver_prefix_counters_restart_on_distinct_loop_edges() {
    assert_prefixes(
        &format!(
            "{NODE}fun run(flag: Boolean): Unit {{ loop {{ val first = Node(); first.touch(\"first\", break) }}; while (flag) {{ val second = Node(); second.touch(\"second\", continue) }} }}"
        ),
        &[
            Prefix {
                call: "first.touch(\"first\", break)",
                operand: "first",
                jump: "break",
                calls_before: 1,
                receivers_before: 0,
                drop_at_jump: true,
            },
            Prefix {
                call: "second.touch(\"second\", continue)",
                operand: "second",
                jump: "continue",
                calls_before: 1,
                receivers_before: 0,
                drop_at_jump: true,
            },
        ],
    );
}

#[test]
fn errors_and_deferred_revoke_a_real_loop_prefix() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        let call = format!("node.touch(\"prefix\", {jump})");
        let prefix = format!(
            "{NODE}fun run(flag: Boolean): Unit {{ {loop_start} {{ val node = Node(); {call} }} }}\n"
        );
        assert_prefixes(
            &prefix,
            &[Prefix {
                call: &call,
                operand: "node",
                jump,
                calls_before: 1,
                receivers_before: 0,
                drop_at_jump: true,
            }],
        );
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
}

#[test]
fn for_body_receiver_prefix_precedes_provider_cleanup_without_changing_it() {
    use lang_frontend::ownership_checking::UnitIterationCleanupAction as Action;
    for jump in ["break", "continue"] {
        let call = format!("node.touch(\"prefix\", {jump})");
        let observed = assert_prefixes(
            &format!(
                "{NODE}fun run(): Unit {{ for (item in listOf(1)) {{ val node = Node(); {call} }} }}"
            ),
            &[Prefix {
                call: &call,
                operand: "node",
                jump,
                calls_before: 1,
                receivers_before: 0,
                drop_at_jump: true,
            }],
        );
        for unit in &observed.units {
            let [event] = unit.receiver_call_prefix_ends() else {
                panic!("one prefix")
            };
            let actions = unit.iteration_cleanup_at(event.point()).unwrap();
            assert!(
                matches!(&actions[0], Action::EndCallLoan(loan) if loan.call() == event.receiver().call())
            );
            assert!(
                matches!(&actions[1], Action::EndReceiverLoan(receiver) if receiver == event.receiver())
            );
            assert!(matches!(&actions[2], Action::Drop(drop) if drop.point() == event.point()));
            assert_eq!(
                actions
                    .iter()
                    .filter(|action| matches!(action, Action::EndReceiverLoan(_)))
                    .count(),
                1
            );
            assert_eq!(
                actions
                    .iter()
                    .any(|action| matches!(action, Action::FinishProvider { .. })),
                jump == "break"
            );
        }
    }
}

#[test]
fn loop_prefixes_preserve_borrowed_temporary_projection_provider_and_reservation_boundaries() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        for text in [
            format!(
                "{NODE}fun run(node: Node, flag: Boolean): Unit {{ {loop_start} {{ node.touch(\"borrowed\", {jump}) }} }}"
            ),
            format!(
                "{NODE}fun run(flag: Boolean): Unit {{ {loop_start} {{ Node().touch(\"temporary\", {jump}) }} }}"
            ),
            format!(
                "{NODE}class Holder(val node: Node)\nfun run(own holder: Holder, flag: Boolean): Unit {{ {loop_start} {{ holder.node.touch(\"field\", {jump}) }} }}"
            ),
            format!(
                "{NODE}fun run(own nodes: List<Node>, flag: Boolean): Unit {{ for (node in nodes) {{ {loop_start} {{ node.touch(\"provider\", {jump}) }} }} }}"
            ),
            format!(
                "class Node {{ inout fun touch(text: String, own next: Int): Int = next }}\nfun run(flag: Boolean): Unit {{ var node = Node(); {loop_start} {{ node.touch(\"reservation\", {jump}) }} }}"
            ),
        ] {
            let observed = assert_prefixes(&text, &[]);
            for unit in &observed.units {
                assert!(
                    !unit.receiver_facts().is_empty(),
                    "receiver descriptor remains real"
                );
            }
        }
    }
}

#[test]
fn original_parameter_loop_sources_now_publish_their_actual_prefix_end() {
    for (body, call, jump) in [
        (
            "loop { node.touch(\"break\", break) }",
            "node.touch(\"break\", break)",
            "break",
        ),
        (
            "while (flag) { node.touch(\"continue\", continue) }",
            "node.touch(\"continue\", continue)",
            "continue",
        ),
    ] {
        assert_prefixes(
            &format!("{NODE}fun run(own node: Node, flag: Boolean): Unit {{ {body} }}"),
            &[Prefix {
                call,
                operand: "node",
                jump,
                calls_before: 1,
                receivers_before: 0,
                // The original parameter belongs to the outer scope, not this loop body.
                drop_at_jump: false,
            }],
        );
    }
}

#[test]
fn counters_restart_for_break_and_continue_branches_in_one_loop() {
    assert_prefixes(
        &format!(
            "{NODE}fun run(flag: Boolean): Unit {{ while (flag) {{ val node = Node(); if (flag) {{ node.touch(\"break\", break) }} else {{ node.touch(\"continue\", continue) }} }} }}"
        ),
        &[
            Prefix {
                call: "node.touch(\"break\", break)",
                operand: "node",
                jump: "break",
                calls_before: 1,
                receivers_before: 0,
                drop_at_jump: true,
            },
            Prefix {
                call: "node.touch(\"continue\", continue)",
                operand: "node",
                jump: "continue",
                calls_before: 1,
                receivers_before: 0,
                drop_at_jump: true,
            },
        ],
    );
}
