//! A Shared receiver reborrow of an Inout parameter never retires its parent capability.
use super::receiver_cfg_prefix::{Prefix, assert_prefixes};
use super::receiver_return::{NODE, assert_returns};
use super::*;
use lang_frontend::ownership_checking::{OwnershipBindingKind, UnitReceiverOwnershipTarget};

fn assert_inout_unowned(observed: &Observed) {
    for unit in &observed.units {
        for receiver in unit
            .receiver_call_return_ends()
            .iter()
            .map(|event| event.receiver())
            .chain(
                unit.receiver_call_prefix_ends()
                    .iter()
                    .map(|event| event.receiver()),
            )
        {
            let UnitReceiverOwnershipTarget::Place(place) = receiver.target() else {
                panic!("Inout parameter root")
            };
            assert_eq!(
                unit.binding_kind(place.root()),
                Some(OwnershipBindingKind::Exclusive)
            );
            assert!(
                unit.drops()
                    .iter()
                    .all(|drop| drop.target() != UnitDropTarget::Named(place.root())),
                "ending a nested Shared token never destroys the caller's owner"
            );
        }
    }
}

fn prefix<'a>(
    call: &'a str,
    operand: &'a str,
    jump: &'a str,
    calls_before: usize,
    receivers_before: usize,
) -> Prefix<'a> {
    Prefix {
        call,
        operand,
        jump,
        calls_before,
        receivers_before,
        drop_at_jump: false,
    }
}

#[test]
fn original_inout_parameter_normal_source_now_observes_its_shared_reborrow_end() {
    // Preserve the exact source originally excluded by borrowed_receiver_return.rs.
    let observed = assert_returns(
        &format!("{NODE}fun run(inout node: Node): Unit {{ node.touch(\"exclusive-root\", 0) }}"),
        &[("node.touch(\"exclusive-root\", 0)", "node")],
    );
    assert_inout_unowned(&observed);
}

#[test]
fn original_inout_parameter_prefix_sources_now_observe_their_shared_reborrow_end() {
    for (opener, closer, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        // Preserve the exact sources originally excluded by borrowed_receiver_prefix.rs.
        let observed = assert_prefixes(
            &format!(
                "{NODE}fun run(inout node: Node, flag: Boolean): Unit {{ {opener} node.touch(\"excluded\", {jump}) {closer} }}"
            ),
            &[prefix(
                &format!("node.touch(\"excluded\", {jump})"),
                "node",
                jump,
                1,
                0,
            )],
        );
        assert_inout_unowned(&observed);
    }
}

#[test]
fn grouped_inout_roots_keep_their_normal_and_prefix_identity() {
    let normal = "((node)).touch(\"normal\", 0)";
    let observed = assert_returns(
        &format!("{NODE}fun run(inout node: Node): Unit {{ {normal} }}"),
        &[(normal, "((node))")],
    );
    assert_inout_unowned(&observed);
    for (opener, closer, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let call = format!("((node)).touch(\"prefix\", {jump})");
        let observed = assert_prefixes(
            &format!(
                "{NODE}fun run(inout node: Node, flag: Boolean): Unit {{ {opener} {call} {closer} }}"
            ),
            &[prefix(&call, "((node))", jump, 1, 0)],
        );
        assert_inout_unowned(&observed);
    }
}

#[test]
fn nested_same_inout_root_keeps_distinct_tokens_and_reverse_unwind() {
    let observed = assert_returns(
        &format!(
            "{NODE}fun run(inout node: Node): Unit {{ node.touch(\"outer\", node.touch(\"inner\", 0)) }}"
        ),
        &[
            ("node.touch(\"inner\", 0)", "node"),
            ("node.touch(\"outer\", node.touch(\"inner\", 0))", "node"),
        ],
    );
    assert_inout_unowned(&observed);
    for (opener, closer, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let inner = format!("node.touch(\"inner\", {jump})");
        let outer = format!("node.touch(\"outer\", {inner})");
        let observed = assert_prefixes(
            &format!(
                "{NODE}fun run(inout node: Node, flag: Boolean): Unit {{ {opener} {outer} {closer} }}"
            ),
            &[
                prefix(&inner, "node", jump, 1, 0),
                prefix(&outer, "node", jump, 2, 1),
            ],
        );
        assert_inout_unowned(&observed);
        for unit in &observed.units {
            let events = unit.receiver_call_prefix_ends();
            assert_ne!(events[0].receiver().call(), events[1].receiver().call());
            assert_eq!(events[0].receiver().target(), events[1].receiver().target());
        }
    }
}

const CELL: &str = "class Cell(var value: Int) { fun read(): Int = value; inout fun set(own next: Int): Unit { value = next } }\n";

#[test]
fn completed_shared_reborrow_preserves_inout_mutation_and_exclusive_receiver_boundary() {
    let observed = assert_returns(
        &format!(
            "{CELL}fun run(inout cell: Cell): Unit {{ val before = cell.read(); cell.set(2); val after = ((cell)).read() }}"
        ),
        &[("cell.read()", "cell"), ("((cell)).read()", "((cell))")],
    );
    assert_inout_unowned(&observed);
    for unit in &observed.units {
        assert!(unit.receiver_call_prefix_ends().is_empty());
        let exclusive: Vec<_> = unit
            .receiver_facts()
            .iter()
            .filter(|receiver| receiver.is_receiver_reservation())
            .collect();
        assert_eq!(exclusive.len(), 1);
        assert!(exclusive[0].activation_point().is_some());
        assert!(
            unit.receiver_call_return_ends()
                .iter()
                .all(|event| event.receiver() != exclusive[0])
        );
    }
}

#[test]
fn caller_retains_its_only_owner_after_inout_wrapper_normal_or_abandoned_reborrow() {
    for (body, normal_count, prefix_count) in
        [("cell.read()", 2, 0), ("cell.readAfter(return)", 1, 1)]
    {
        let text = format!(
            "class Cell(var value: Int) {{ fun read(): Int = value; fun readAfter(own next: Unit): Int = value; inout fun set(own next: Int): Unit {{ value = next }} }}\nfun wrapper(inout cell: Cell): Unit {{ {body} }}\nfun run(): Unit {{ var cell = Cell(1); wrapper(&cell); cell.set(2); cell.read() }}"
        );
        let observed = observe(&text);
        assert_units_admitted(&observed);
        for unit in &observed.units {
            assert_eq!(unit.receiver_call_return_ends().len(), normal_count);
            assert_eq!(unit.receiver_call_prefix_ends().len(), prefix_count);
            let caller = unit.receiver_call_return_ends().last().unwrap().receiver();
            let UnitReceiverOwnershipTarget::Place(owned) = caller.target() else {
                panic!("caller root")
            };
            assert!(unit.formed_owned_locals().contains(&owned.root()));
            let callee = if prefix_count == 1 {
                unit.receiver_call_prefix_ends()[0].receiver()
            } else {
                unit.receiver_call_return_ends()[0].receiver()
            };
            let UnitReceiverOwnershipTarget::Place(parameter) = callee.target() else {
                panic!("callee parameter")
            };
            assert_eq!(
                unit.binding_kind(parameter.root()),
                Some(OwnershipBindingKind::Exclusive)
            );
            assert_ne!(parameter.root(), owned.root());
            assert!(
                unit.drops()
                    .iter()
                    .all(|drop| drop.target() != UnitDropTarget::Named(parameter.root()))
            );
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Named(owned.root()))
                .collect();
            assert_eq!(drops.len(), 1);
            assert_eq!(drops[0].point(), UnitDropPoint::CallReturn(caller.call()));
        }
    }
}

#[test]
fn exclusive_receiver_reservation_stays_excluded_but_counts_in_real_prefix_order() {
    use lang_frontend::ownership_checking::UnitReceiverOwnershipKind;
    let mutable = "class Mutable { inout fun change(text: String, own next: Int): Int = next }\n";
    for (opener, closer, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let inner = format!("mutable.change(\"inner\", {jump})");
        let outer = format!("node.touch(\"outer\", {inner})");
        let observed = assert_prefixes(
            &format!(
                "{NODE}{mutable}fun run(inout node: Node, inout mutable: Mutable, flag: Boolean): Unit {{ {opener} {outer} {closer} }}"
            ),
            &[prefix(&outer, "node", jump, 2, 1)],
        );
        assert_inout_unowned(&observed);
        for unit in &observed.units {
            let exclusive: Vec<_> = unit
                .receiver_facts()
                .iter()
                .filter(|receiver| receiver.kind() == UnitReceiverOwnershipKind::ExclusiveLoan)
                .collect();
            assert_eq!(exclusive.len(), 1);
            assert!(exclusive[0].is_receiver_reservation());
            assert!(exclusive[0].activation_point().is_none());
        }
    }
}

#[test]
fn completed_inner_shared_token_is_not_repeated_by_outer_inout_prefix() {
    let node = "class Node { fun ready(): String = \"ready\"; fun touch(text: String, own next: Int): Int = next }\n";
    for (opener, closer, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let outer = format!("node.touch(node.ready(), {jump})");
        let observed = assert_prefixes(
            &format!(
                "{node}fun run(inout node: Node, flag: Boolean): Unit {{ {opener} {outer} {closer} }}"
            ),
            &[prefix(&outer, "node", jump, 1, 0)],
        );
        assert_inout_unowned(&observed);
        for unit in &observed.units {
            let [completed] = unit.receiver_call_return_ends() else {
                panic!("only completed inner")
            };
            let call = observed
                .parsed
                .ast()
                .expressions()
                .get(completed.receiver().call().expression())
                .unwrap();
            assert_eq!(observed.sources.slice(call.span()), Ok("node.ready()"));
            assert_ne!(
                completed.receiver().call(),
                unit.receiver_call_prefix_ends()[0].receiver().call()
            );
        }
    }
}

#[test]
fn inner_loop_jump_preserves_outer_inout_reborrow_and_later_mutation() {
    let node = "class Node { fun touch(text: String, own next: Int): Int = next; inout fun update(): Unit {} }\n";
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        let inner = format!("node.touch(\"inner\", {jump})");
        let outer = format!(
            "node.touch(\"outer\", if (flag) {{ {loop_start} {{ {inner} }}; 0 }} else {{ 0 }})"
        );
        let observed = assert_prefixes(
            &format!(
                "{node}fun run(inout node: Node, flag: Boolean): Unit {{ {outer}; node.update() }}"
            ),
            &[prefix(&inner, "node", jump, 1, 0)],
        );
        assert_inout_unowned(&observed);
        for unit in &observed.units {
            let [completed] = unit.receiver_call_return_ends() else {
                panic!("outer normal continuation")
            };
            let call = observed
                .parsed
                .ast()
                .expressions()
                .get(completed.receiver().call().expression())
                .unwrap();
            assert_eq!(observed.sources.slice(call.span()), Ok(outer.as_str()));
            let reservations: Vec<_> = unit
                .receiver_facts()
                .iter()
                .filter(|receiver| receiver.is_receiver_reservation())
                .collect();
            assert_eq!(reservations.len(), 1);
            assert!(reservations[0].activation_point().is_some());
        }
    }
}

#[test]
fn inout_parameter_reborrow_keeps_actual_for_cleanup_and_no_parameter_drop() {
    use lang_frontend::ownership_checking::UnitIterationCleanupAction as Action;
    for jump in ["return", "break", "continue"] {
        let call = format!("node.touch(\"prefix\", {jump})");
        let observed = assert_prefixes(
            &format!(
                "{NODE}fun run(inout node: Node): Unit {{ for (item in listOf(1)) {{ {call} }} }}"
            ),
            &[prefix(&call, "node", jump, 1, 0)],
        );
        assert_inout_unowned(&observed);
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
                jump != "continue"
            );
        }
    }
}

#[test]
fn projected_provider_bound_and_borrowed_result_receivers_stay_excluded() {
    for next in ["0", "return"] {
        for text in [
            format!(
                "{NODE}class Holder(val node: Node)\nfun run(inout holder: Holder): Unit {{ holder.node.touch(\"field\", {next}) }}"
            ),
            format!(
                "{NODE}fun run(inout nodes: List<Node>): Unit {{ for (node in nodes) {{ node.touch(\"provider\", {next}) }} }}"
            ),
            format!(
                "{NODE}fun alias(source: Node): borrow Node from source = source\nfun run(inout node: Node): Unit {{ borrow val borrowed = alias(node); borrowed.touch(\"binding\", {next}) }}"
            ),
            format!(
                "class Node {{ fun view(source: Node, own next: Int): borrow Node from source = source }}\nfun run(inout node: Node, source: Node): Unit {{ borrow val result = node.view(source, {next}) }}"
            ),
        ] {
            let observed = observe(&text);
            assert_units_admitted(&observed);
            for unit in &observed.units {
                assert!(!unit.receiver_facts().is_empty());
                assert!(unit.receiver_call_return_ends().is_empty());
                assert!(unit.receiver_call_prefix_ends().is_empty());
            }
        }
    }
}

#[test]
fn shared_reborrow_completion_does_not_allow_moving_out_of_the_inout_parent() {
    let text = format!(
        "{NODE}fun consume(own item: Node): Unit {{}}\nfun run(inout node: Node): Unit {{ node.touch(\"before\", 0); consume(node) }}"
    );
    let observed = observe(&text);
    for unit in &observed.units {
        assert!(
            unit.diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0133")
        );
        assert!(unit.receiver_call_return_ends().is_empty());
        assert!(unit.receiver_call_prefix_ends().is_empty());
        assert!(unit.clone().validate().is_err());
    }
}

#[test]
fn later_errors_and_deferred_revoke_nonempty_inout_reborrow_end_tables() {
    for next in ["0", "return"] {
        let call = format!("node.touch(\"ready\", {next})");
        let source = format!("{NODE}fun run(inout node: Node): Unit {{ {call} }}\n");
        if next == "0" {
            assert_returns(&source, &[(&call, "node")]);
        } else {
            assert_prefixes(&source, &[prefix(&call, "node", "return", 1, 0)]);
        }
        for (suffix, deferred) in [
            ("fun rejected(): Unit { val target = 0; target = 1 }", false),
            (
                "fun rejected(): Unit { var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 }) }",
                true,
            ),
        ] {
            let observed = observe(&format!("{source}{suffix}"));
            for unit in &observed.units {
                if deferred {
                    assert!(unit.diagnostics().is_empty());
                    assert!(!unit.deferred().is_empty());
                } else {
                    assert!(
                        unit.diagnostics()
                            .iter()
                            .any(|diagnostic| diagnostic.code().to_string() == "L0134")
                    );
                }
                assert!(unit.receiver_call_prefix_ends().is_empty());
                assert!(unit.receiver_call_return_ends().is_empty());
                assert!(unit.clone().validate().is_err());
            }
        }
    }
}
