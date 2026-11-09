//! End only the abandoned nested reborrow, never the caller's parameter loan or owner.
use super::receiver_cfg_prefix::{Prefix, assert_prefixes};
use super::receiver_return::NODE;
use super::*;
use lang_frontend::ownership_checking::{OwnershipBindingKind, UnitReceiverOwnershipTarget};

fn assert_parameter_unowned(observed: &Observed) {
    for unit in &observed.units {
        for event in unit.receiver_call_prefix_ends() {
            let UnitReceiverOwnershipTarget::Place(place) = event.receiver().target() else {
                panic!("parameter root")
            };
            assert_eq!(
                unit.binding_kind(place.root()),
                Some(OwnershipBindingKind::Shared)
            );
            assert!(
                unit.drops()
                    .iter()
                    .all(|drop| drop.target() != UnitDropTarget::Named(place.root())),
                "abandoned reborrow never destroys the caller's owner"
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
fn original_borrowed_receiver_return_source_now_observes_its_abandoned_reborrow() {
    // Preserve the exact source originally excluded by receiver_prefix.rs.
    let observed = assert_prefixes(
        &format!("{NODE}fun run(node: Node): Unit {{ node.touch(\"borrowed\", return) }}"),
        &[prefix(
            "node.touch(\"borrowed\", return)",
            "node",
            "return",
            1,
            0,
        )],
    );
    assert_parameter_unowned(&observed);
}

#[test]
fn original_borrowed_loop_sources_now_observe_their_abandoned_reborrow() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        // Preserve the exact sources originally excluded by receiver_cfg_prefix.rs.
        let observed = assert_prefixes(
            &format!(
                "{NODE}fun run(node: Node, flag: Boolean): Unit {{ {loop_start} {{ node.touch(\"borrowed\", {jump}) }} }}"
            ),
            &[prefix(
                &format!("node.touch(\"borrowed\", {jump})"),
                "node",
                jump,
                1,
                0,
            )],
        );
        assert_parameter_unowned(&observed);
    }
}

#[test]
fn default_explicit_and_grouped_borrow_parameters_end_only_the_nested_token() {
    for (opener, closer, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        for marker in ["", "borrow "] {
            for operand in ["node", "((node))"] {
                let call = format!("{operand}.touch(\"prefix\", {jump})");
                let observed = assert_prefixes(
                    &format!(
                        "{NODE}fun run({marker}node: Node, flag: Boolean): Unit {{ {opener} {call} {closer} }}"
                    ),
                    &[prefix(&call, operand, jump, 1, 0)],
                );
                assert_parameter_unowned(&observed);
                for unit in &observed.units {
                    assert!(unit.receiver_call_return_ends().is_empty());
                }
            }
        }
    }
}

#[test]
fn nested_same_parameter_keeps_each_real_token_in_reverse_order() {
    for (opener, closer, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let inner = format!("node.touch(\"inner\", {jump})");
        let outer = format!("node.touch(\"outer\", {inner})");
        let observed = assert_prefixes(
            &format!(
                "{NODE}fun run(node: Node, flag: Boolean): Unit {{ {opener} {outer} {closer} }}"
            ),
            &[
                prefix(&inner, "node", jump, 1, 0),
                prefix(&outer, "node", jump, 2, 1),
            ],
        );
        assert_parameter_unowned(&observed);
        for unit in &observed.units {
            let events = unit.receiver_call_prefix_ends();
            assert_ne!(events[0].receiver().call(), events[1].receiver().call());
            assert_eq!(events[0].receiver().target(), events[1].receiver().target());
        }
    }
}

#[test]
fn nested_loop_jump_does_not_end_the_outer_parameter_reborrow() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        let inner = format!("inner.touch(\"inner\", {jump})");
        let outer = format!(
            "outer.touch(\"outer\", if (flag) {{ {loop_start} {{ {inner} }}; 0 }} else {{ 0 }})"
        );
        let observed = assert_prefixes(
            &format!("{NODE}fun run(outer: Node, inner: Node, flag: Boolean): Unit {{ {outer} }}"),
            &[prefix(&inner, "inner", jump, 1, 0)],
        );
        assert_parameter_unowned(&observed);
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
            assert_ne!(
                completed.receiver().call(),
                unit.receiver_call_prefix_ends()[0].receiver().call()
            );
        }
    }
}

#[test]
fn parameter_reborrow_precedes_real_for_cleanup_without_a_parameter_drop() {
    use lang_frontend::ownership_checking::UnitIterationCleanupAction as Action;
    for jump in ["return", "break", "continue"] {
        let call = format!("node.touch(\"prefix\", {jump})");
        let observed = assert_prefixes(
            &format!("{NODE}fun run(node: Node): Unit {{ for (item in listOf(1)) {{ {call} }} }}"),
            &[prefix(&call, "node", jump, 1, 0)],
        );
        assert_parameter_unowned(&observed);
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
fn loop_jump_releases_only_this_reborrow_and_parameter_remains_usable() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        let call = format!("node.touch(\"prefix\", {jump})");
        let observed = assert_prefixes(
            &format!(
                "{NODE}fun run(node: Node, flag: Boolean): Unit {{ {loop_start} {{ {call} }}; node.touch(\"after\", 0) }}"
            ),
            &[prefix(&call, "node", jump, 1, 0)],
        );
        assert_parameter_unowned(&observed);
        for unit in &observed.units {
            let [after] = unit.receiver_call_return_ends() else {
                panic!("post-loop use")
            };
            let [before] = unit.receiver_call_prefix_ends() else {
                panic!("abandoned call")
            };
            assert_eq!(after.receiver().target(), before.receiver().target());
            assert_ne!(after.receiver().call(), before.receiver().call());
            let node = observed
                .parsed
                .ast()
                .expressions()
                .get(after.receiver().call().expression())
                .unwrap();
            assert_eq!(
                observed.sources.slice(node.span()),
                Ok("node.touch(\"after\", 0)")
            );
        }
    }
}

#[test]
fn original_borrow_and_inout_parameter_source_observes_both_shared_reborrows() {
    for (opener, closer, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let inner = format!("excluded.touch(\"inner\", {jump})");
        let outer = format!("node.touch(\"outer\", {inner})");
        let observed = assert_prefixes(
            &format!(
                "{NODE}fun run(node: Node, inout excluded: Node, flag: Boolean): Unit {{ {opener} {outer} {closer} }}"
            ),
            &[
                prefix(&inner, "excluded", jump, 1, 0),
                prefix(&outer, "node", jump, 2, 1),
            ],
        );
        for unit in &observed.units {
            assert_eq!(unit.receiver_facts().len(), 2);
            for (event, kind) in unit.receiver_call_prefix_ends().iter().zip([
                OwnershipBindingKind::Exclusive,
                OwnershipBindingKind::Shared,
            ]) {
                let UnitReceiverOwnershipTarget::Place(place) = event.receiver().target() else {
                    panic!("parameter root")
                };
                assert_eq!(unit.binding_kind(place.root()), Some(kind));
                assert!(
                    unit.drops()
                        .iter()
                        .all(|drop| drop.target() != UnitDropTarget::Named(place.root()))
                );
            }
        }
    }
}

#[test]
fn distinct_edges_restart_parameter_reborrow_counters() {
    let observed = assert_prefixes(
        &format!(
            "{NODE}fun run(node: Node, flag: Boolean): Unit {{ while (flag) {{ if (flag) {{ node.touch(\"break\", break) }} else {{ node.touch(\"continue\", continue) }} }} }}"
        ),
        &[
            prefix("node.touch(\"break\", break)", "node", "break", 1, 0),
            prefix(
                "node.touch(\"continue\", continue)",
                "node",
                "continue",
                1,
                0,
            ),
        ],
    );
    assert_parameter_unowned(&observed);
    for unit in &observed.units {
        assert_ne!(
            unit.receiver_call_prefix_ends()[0].point(),
            unit.receiver_call_prefix_ends()[1].point()
        );
    }
}

#[test]
fn original_provenance_exclusions_still_hold_for_parameter_roots() {
    for (opener, closer, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let mut texts = vec![
            format!(
                "{NODE}fun alias(source: Node): borrow Node from source = source\nfun run(node: Node, flag: Boolean): Unit {{ borrow val borrowed = alias(node); {opener} borrowed.touch(\"binding\", {jump}) {closer} }}"
            ),
            format!(
                "{NODE}fun run(nodes: List<Node>, flag: Boolean): Unit {{ for (node in nodes) {{ {opener} node.touch(\"provider\", {jump}) {closer} }} }}"
            ),
        ];
        if jump == "return" {
            texts.push("class Node { fun view(source: Node, own next: Int): borrow Node from source = source }\nfun run(node: Node, source: Node, flag: Boolean): Unit { borrow val unused = node.view(source, return) }".into());
        }
        for text in texts {
            let observed = assert_prefixes(&text, &[]);
            for unit in &observed.units {
                assert!(!unit.receiver_facts().is_empty());
            }
        }
    }
}

#[test]
fn unproven_borrow_result_loop_continuations_remain_structured_rejections() {
    for (opener, jump) in [("loop {", "break"), ("while (flag) {", "continue")] {
        for text in [
            format!(
                "{NODE}fun alias(source: Node): borrow Node from source = source\nfun run(node: Node, flag: Boolean): Unit {{ {opener} borrow val borrowed = alias(node); borrowed.touch(\"binding\", {jump}) }} }}"
            ),
            format!(
                "class Node {{ fun view(source: Node, own next: Int): borrow Node from source = source }}\nfun run(node: Node, source: Node, flag: Boolean): Unit {{ {opener} borrow val unused = node.view(source, {jump}) }} }}"
            ),
        ] {
            let observed = observe(&text);
            for unit in &observed.units {
                let [diagnostic] = unit.diagnostics() else {
                    panic!("one unchanged loop continuation rejection: {text}")
                };
                assert_eq!(diagnostic.code().to_string(), "L0164");
                assert_eq!(
                    diagnostic.message(),
                    "borrow result loop continuation is not yet proven"
                );
                let start = text.find("borrow val").unwrap();
                assert_eq!(
                    (
                        diagnostic.primary_span().start(),
                        diagnostic.primary_span().end()
                    ),
                    (start, start + "borrow".len())
                );
                assert!(unit.receiver_call_prefix_ends().is_empty());
                assert!(unit.clone().validate().is_err());
            }
        }
    }
}

#[test]
fn unreachable_and_inner_loop_transfers_do_not_invent_parameter_prefix_ends() {
    for body in [
        "return; node.touch(\"unvisited\", 0)",
        "node.touch(\"normal\", 0); return",
        "node.touch(\"outer\", if (flag) { loop { break }; 0 } else { 0 })",
    ] {
        let observed = assert_prefixes(
            &format!("{NODE}fun run(node: Node, flag: Boolean): Unit {{ {body} }}"),
            &[],
        );
        assert_parameter_unowned(&observed);
    }
}

#[test]
fn later_error_and_deferred_revoke_each_nonempty_parameter_prefix() {
    for (opener, closer, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let call = format!("node.touch(\"prefix\", {jump})");
        let source = format!(
            "{NODE}fun run(node: Node, flag: Boolean): Unit {{ {opener} {call} {closer} }}\n"
        );
        assert_prefixes(&source, &[prefix(&call, "node", jump, 1, 0)]);
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
                            .any(|d| d.code().to_string() == "L0134")
                    );
                }
                assert!(unit.receiver_call_prefix_ends().is_empty());
                assert!(unit.clone().validate().is_err());
            }
        }
    }
}
