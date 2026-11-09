//! Normal Shared receiver completion remains separate from abandoned prefixes.
use super::*;
use lang_frontend::{
    ownership_checking::{UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget},
    type_checking::UnitCallReceiverOrigin,
};

pub(super) const NODE: &str = "class Node { fun touch(text: String, own next: Int): Int = next }\n";

pub(super) fn assert_returns(text: &str, expected: &[(&str, &str)]) -> Observed {
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
        let events = unit.receiver_call_return_ends();
        assert_eq!(
            events.len(),
            expected.len(),
            "complete bounded vector: {text}"
        );
        for (event, &(call_text, operand_text)) in events.iter().zip(expected) {
            let receiver = event.receiver();
            assert!(unit.receiver_facts().contains(receiver));
            assert_eq!(receiver.kind(), UnitReceiverOwnershipKind::SharedLoan);
            assert!(!receiver.is_receiver_reservation());
            let call = node(receiver.call());
            assert_eq!(observed.sources.slice(call.span()), Ok(call_text));
            let start = text.find(call_text).unwrap();
            assert_eq!(
                (call.span().start(), call.span().end()),
                (start, start + call_text.len())
            );
            assert_eq!(event.point(), UnitDropPoint::CallReturn(receiver.call()));
            assert_eq!(receiver.end_span(), call.span());
            let UnitCallReceiverOrigin::Expression(operand) = receiver.source() else {
                panic!("explicit receiver")
            };
            assert_eq!(
                observed.sources.slice(node(operand).span()),
                Ok(operand_text)
            );
            assert_eq!(receiver.begin_span(), node(operand).span());
            let UnitReceiverOwnershipTarget::Place(place) = receiver.target() else {
                panic!("owned root")
            };
            assert!(place.is_root());
            assert_eq!(place.root().source_unit(), source.id());
            assert_eq!(
                local.resolution().symbols()[place.root().symbol().index()].name(),
                operand_text.trim_matches(['(', ')'])
            );
        }
    }
    observed
}

#[test]
fn owned_name_and_group_receiver_complete_at_the_actual_call_return() {
    for operand in ["node", "((node))"] {
        let call = format!("{operand}.touch(\"ready\", 0)");
        let observed = assert_returns(
            &format!("{NODE}fun run(own node: Node): Unit {{ {call} }}"),
            &[(&call, operand)],
        );
        for unit in &observed.units {
            assert!(unit.receiver_call_prefix_ends().is_empty());
            let [event] = unit.receiver_call_return_ends() else {
                panic!("one completed receiver")
            };
            let UnitReceiverOwnershipTarget::Place(place) = event.receiver().target() else {
                panic!("root")
            };
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
                .collect();
            assert_eq!(drops.len(), 1);
            assert_eq!(drops[0].point(), event.point());
        }
    }
}

#[test]
fn nested_completed_receivers_keep_their_distinct_tokens_until_each_return() {
    let inner = "node.touch(\"inner\", 0)";
    let outer = format!("node.touch(\"outer\", {inner})");
    let observed = assert_returns(
        &format!("{NODE}fun run(own node: Node): Unit {{ {outer} }}"),
        &[(inner, "node"), (&outer, "node")],
    );
    for unit in &observed.units {
        assert!(unit.receiver_call_prefix_ends().is_empty());
        let [inner, outer] = unit.receiver_call_return_ends() else {
            panic!("two tokens")
        };
        assert_eq!(inner.receiver().target(), outer.receiver().target());
        let UnitReceiverOwnershipTarget::Place(place) = outer.receiver().target() else {
            panic!("root")
        };
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            .collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(
            drops[0].point(),
            outer.point(),
            "inner completion does not release the outer pending root"
        );
    }
}

#[test]
fn completed_inner_receiver_does_not_reappear_when_outer_prefix_unwinds() {
    let node = "class Node { fun ready(): String = \"ready\"; fun touch(text: String, own next: Int): Int = next }\n";
    for (opener, closer, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let text = format!(
            "{node}fun run(flag: Boolean): Unit {{ {opener} val node = Node(); node.touch(node.ready(), {jump}) {closer} }}"
        );
        let observed = assert_returns(&text, &[("node.ready()", "node")]);
        for unit in &observed.units {
            let [normal] = unit.receiver_call_return_ends() else {
                panic!("only completed inner call")
            };
            let [prefix] = unit.receiver_call_prefix_ends() else {
                panic!("only abandoned outer call")
            };
            assert_ne!(normal.receiver().call(), prefix.receiver().call());
            assert_eq!(normal.receiver().target(), prefix.receiver().target());
            assert!(matches!(prefix.point(), UnitDropPoint::ControlTransfer(_)));
            assert_eq!(
                prefix.preceding_receiver_loan_ends(),
                0,
                "completed inner token was already removed"
            );
            assert!(
                unit.drops()
                    .iter()
                    .all(|drop| drop.point() != normal.point()),
                "outer pending receiver keeps the same owner alive"
            );
        }
    }
}

#[test]
fn nested_loop_prefix_exit_preserves_the_outer_normal_receiver_completion() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        let outer = format!(
            "outer.touch(\"outer\", if (flag) {{ {loop_start} {{ val inner = Node(); inner.touch(\"inner\", {jump}) }}; 0 }} else {{ 0 }})"
        );
        let observed = assert_returns(
            &format!("{NODE}fun run(flag: Boolean): Unit {{ val outer = Node(); {outer} }}"),
            &[(&outer, "outer")],
        );
        for unit in &observed.units {
            assert_eq!(unit.receiver_call_prefix_ends().len(), 1);
            assert_ne!(
                unit.receiver_call_prefix_ends()[0].receiver().target(),
                unit.receiver_call_return_ends()[0].receiver().target()
            );
        }
    }
}

#[test]
fn branch_and_loop_returns_are_static_observations_not_dynamic_counts() {
    assert_returns(
        &format!(
            "{NODE}fun run(own node: Node, flag: Boolean): Unit {{ if (flag) {{ node.touch(\"left\", 0) }} else {{ node.touch(\"right\", 0) }} }}"
        ),
        &[
            ("node.touch(\"left\", 0)", "node"),
            ("node.touch(\"right\", 0)", "node"),
        ],
    );
    assert_returns(
        &format!(
            "{NODE}fun run(own node: Node, flag: Boolean): Unit {{ while (flag) {{ node.touch(\"repeat\", 0); continue }}; node.touch(\"done\", 0) }}"
        ),
        &[
            ("node.touch(\"repeat\", 0)", "node"),
            ("node.touch(\"done\", 0)", "node"),
        ],
    );
}

#[test]
fn zero_argument_normal_receiver_has_its_own_real_token() {
    assert_returns(
        "class Node { fun read(): Int = 1 }\nfun run(own node: Node): Unit { node.read() }",
        &[("node.read()", "node")],
    );
}

#[test]
fn abandoned_and_unreachable_calls_do_not_publish_normal_receiver_ends() {
    for body in [
        "node.touch(\"return\", return)",
        "loop { node.touch(\"break\", break) }",
        "while (flag) { node.touch(\"continue\", continue) }",
        "return; node.touch(\"unreachable\", 0)",
    ] {
        assert_returns(
            &format!("{NODE}fun run(own node: Node, flag: Boolean): Unit {{ {body} }}"),
            &[],
        );
    }
}

#[test]
fn normal_completion_keeps_all_existing_receiver_provenance_boundaries() {
    for text in [
        format!("{NODE}fun run(): Unit {{ Node().touch(\"temporary\", 0) }}"),
        format!("{NODE}fun run(own nodes: List<Node>): Unit {{ for (node in nodes) {{ node.touch(\"provider\", 0) }} }}"),
        format!("{NODE}fun alias(source: Node): borrow Node from source = source\nfun run(own node: Node): Unit {{ borrow val borrowed = alias(node); borrowed.touch(\"binding\", 0) }}"),
        "class Node { inout fun touch(text: String, own next: Int): Int = next }\nfun run(): Unit { var node = Node(); node.touch(\"reservation\", 0) }".into(),
        "class Node { fun view(source: Node): borrow Node from source = source }\nfun run(own node: Node, source: Node): Unit { borrow val unused = node.view(source) }".into(),
    ] {
        let observed = assert_returns(&text, &[]);
        for unit in &observed.units { assert!(!unit.receiver_facts().is_empty()); }
    }
    assert_returns(
        "fun run(): Unit { val source = \"text\"; val copy = source.clone() }",
        &[],
    );
}

#[test]
fn for_body_normal_return_does_not_add_or_duplicate_iteration_cleanup() {
    use lang_frontend::ownership_checking::UnitIterationCleanupAction as Action;
    let observed = assert_returns(
        &format!(
            "{NODE}fun run(): Unit {{ for (item in listOf(1)) {{ val node = Node(); node.touch(\"ready\", 0); break }} }}"
        ),
        &[("node.touch(\"ready\", 0)", "node")],
    );
    for unit in &observed.units {
        let [normal] = unit.receiver_call_return_ends() else {
            panic!("one normal completion")
        };
        assert!(unit.iteration_cleanup_at(normal.point()).is_none());
        assert!(unit.receiver_call_prefix_ends().is_empty());
        for plan in unit.iterations() {
            for exit in plan.exits() {
                assert!(
                    exit.actions()
                        .iter()
                        .all(|action| !matches!(action, Action::EndReceiverLoan(_)))
                );
            }
        }
    }
}

#[test]
fn later_diagnostic_or_deferred_revokes_nonempty_normal_receiver_ends() {
    let prefix = format!("{NODE}fun run(own node: Node): Unit {{ node.touch(\"ready\", 0) }}\n");
    assert_returns(&prefix, &[("node.touch(\"ready\", 0)", "node")]);
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
            assert!(unit.receiver_call_return_ends().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}

#[test]
fn one_receiver_can_end_on_mutually_exclusive_unwind_and_normal_paths() {
    let call = "node.touch(\"prefix\", if (flag) { return } else { 0 })";
    let observed = assert_returns(
        &format!("{NODE}fun run(own node: Node, flag: Boolean): Unit {{ {call} }}"),
        &[(call, "node")],
    );
    for unit in &observed.units {
        let [normal] = unit.receiver_call_return_ends() else {
            panic!("normal successor")
        };
        let [prefix] = unit.receiver_call_prefix_ends() else {
            panic!("return successor")
        };
        assert_eq!(
            normal.receiver(),
            prefix.receiver(),
            "same pending identity on mutually exclusive paths"
        );
        assert_ne!(normal.point(), prefix.point());
        let UnitReceiverOwnershipTarget::Place(place) = normal.receiver().target() else {
            panic!("root")
        };
        let points: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            .map(|drop| drop.point())
            .collect();
        assert_eq!(points, [prefix.point(), normal.point()]);
    }
}

// Compile locally; the existing host policy reserves Abort-containing tests for official CI.
#[test]
fn abort_after_completed_inner_receiver_does_not_invent_outer_return() {
    let observed = assert_returns(
        "class Node { fun ready(): String = \"ready\"; fun touch(text: String, own next: Int): Int = next }\nfun run(own node: Node): Unit { node.touch(node.ready(), error(\"stop\")) }",
        &[("node.ready()", "node")],
    );
    for unit in &observed.units {
        assert!(unit.receiver_call_prefix_ends().is_empty());
        assert!(unit.drops().is_empty());
    }
}
