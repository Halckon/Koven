//! A completed shared reborrow does not transfer or destroy the caller's owner.
use super::receiver_return::{NODE, assert_returns};
use super::*;
use lang_frontend::ownership_checking::{OwnershipBindingKind, UnitReceiverOwnershipTarget};

fn assert_parameter_unowned(observed: &Observed) {
    for unit in &observed.units {
        for event in unit.receiver_call_return_ends() {
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
                "receiver end never destroys the caller's owner"
            );
        }
    }
}

#[test]
fn original_borrowed_receiver_source_now_observes_its_normal_reborrow_end() {
    // Preserve the exact original excluded source from receiver_return.rs.
    let observed = assert_returns(
        &format!("{NODE}fun run(node: Node): Unit {{ node.touch(\"borrowed\", 0) }}"),
        &[("node.touch(\"borrowed\", 0)", "node")],
    );
    assert_parameter_unowned(&observed);
}

#[test]
fn default_and_explicit_borrow_parameters_keep_group_identity_without_a_drop() {
    for marker in ["", "borrow "] {
        for operand in ["node", "((node))"] {
            let call = format!("{operand}.touch(\"ready\", 0)");
            let observed = assert_returns(
                &format!("{NODE}fun run({marker}node: Node): Unit {{ {call} }}"),
                &[(&call, operand)],
            );
            assert_parameter_unowned(&observed);
        }
    }
}

#[test]
fn nested_and_repeated_reborrows_keep_distinct_call_tokens() {
    let inner = "node.touch(\"inner\", 0)";
    let outer = format!("node.touch(\"outer\", {inner})");
    let observed = assert_returns(
        &format!("{NODE}fun run(node: Node): Unit {{ {outer}; node.touch(\"again\", 0) }}"),
        &[
            (inner, "node"),
            (&outer, "node"),
            ("node.touch(\"again\", 0)", "node"),
        ],
    );
    assert_parameter_unowned(&observed);
    for unit in &observed.units {
        assert!(unit.receiver_call_prefix_ends().is_empty());
        let events = unit.receiver_call_return_ends();
        assert!(
            events
                .windows(2)
                .all(|pair| pair[0].receiver().call() != pair[1].receiver().call())
        );
        assert!(
            events
                .iter()
                .all(|event| event.receiver().target() == events[0].receiver().target())
        );
    }
}

#[test]
fn completed_parameter_reborrow_is_not_repeated_in_abandoned_outer_prefixes() {
    let node = "class Node { fun ready(): String = \"ready\"; fun touch(text: String, own next: Int): Int = next }\n";
    for (opener, closer, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let observed = assert_returns(
            &format!(
                "{node}fun run(node: Node, flag: Boolean): Unit {{ {opener} node.touch(node.ready(), {jump}) {closer} }}"
            ),
            &[("node.ready()", "node")],
        );
        assert_parameter_unowned(&observed);
        for unit in &observed.units {
            let [prefix] = unit.receiver_call_prefix_ends() else {
                panic!("only the abandoned outer receiver")
            };
            let outer = observed
                .parsed
                .ast()
                .expressions()
                .get(prefix.receiver().call().expression())
                .unwrap();
            assert_eq!(
                observed.sources.slice(outer.span()),
                Ok(format!("node.touch(node.ready(), {jump})").as_str())
            );
            assert_ne!(
                prefix.receiver().call(),
                unit.receiver_call_return_ends()[0].receiver().call()
            );
            assert_eq!(
                prefix.preceding_receiver_loan_ends(),
                0,
                "completed inner token is already retired"
            );
        }
    }
}

#[test]
fn caller_still_owns_and_uses_the_value_after_wrapper_reborrow_completion() {
    let observed = assert_returns(
        &format!(
            "{NODE}fun wrapper(node: Node): Unit {{ node.touch(\"inside\", 0) }}\nfun run(own node: Node): Unit {{ wrapper(node); node.touch(\"after\", 0) }}"
        ),
        &[
            ("node.touch(\"inside\", 0)", "node"),
            ("node.touch(\"after\", 0)", "node"),
        ],
    );
    for unit in &observed.units {
        let [inside, after] = unit.receiver_call_return_ends() else {
            panic!("wrapper and caller")
        };
        let UnitReceiverOwnershipTarget::Place(borrowed) = inside.receiver().target() else {
            panic!("parameter")
        };
        let UnitReceiverOwnershipTarget::Place(owned) = after.receiver().target() else {
            panic!("caller owner")
        };
        assert_eq!(
            unit.binding_kind(borrowed.root()),
            Some(OwnershipBindingKind::Shared)
        );
        assert_eq!(
            unit.binding_kind(owned.root()),
            Some(OwnershipBindingKind::Owned)
        );
        assert_ne!(
            borrowed.root(),
            owned.root(),
            "static callee and caller identities are not dynamic aliases"
        );
        assert!(
            unit.drops()
                .iter()
                .all(|drop| drop.target() != UnitDropTarget::Named(borrowed.root()))
        );
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(owned.root()))
            .collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), after.point());
    }
}

#[test]
fn borrowed_parameter_cfg_has_only_the_actual_normal_static_successors() {
    let mixed = "node.touch(\"mixed\", if (flag) { return } else { 0 })";
    let observed = assert_returns(
        &format!(
            "{NODE}fun run(node: Node, flag: Boolean): Unit {{ while (flag) {{ node.touch(\"repeat\", 0); continue }}; {mixed} }}"
        ),
        &[("node.touch(\"repeat\", 0)", "node"), (mixed, "node")],
    );
    assert_parameter_unowned(&observed);
    for unit in &observed.units {
        let [prefix] = unit.receiver_call_prefix_ends() else {
            panic!("only the mixed call's Return path")
        };
        assert_eq!(
            prefix.receiver(),
            unit.receiver_call_return_ends()[1].receiver()
        );
        assert!(matches!(prefix.point(), UnitDropPoint::ControlTransfer(_)));
    }
}

#[test]
fn borrowed_loop_bindings_remain_excluded() {
    let text = format!(
        "{NODE}fun run(nodes: List<Node>): Unit {{ for (node in nodes) {{ node.touch(\"provider-binding\", 0) }} }}"
    );
    let observed = assert_returns(&text, &[]);
    for unit in &observed.units {
        assert!(
            !unit.receiver_facts().is_empty(),
            "real receiver descriptors remain"
        );
    }
}

#[test]
fn shared_resource_parameter_and_captured_parameter_never_gain_an_owner_drop() {
    for text in [
        "class Resource { fun read(): Int = 1; deinit() {} }\nfun run(resource: Resource): Unit { resource.read() }",
        "class Resource { fun read(): Int = 1; deinit() {} }\nfun run(resource: Resource): Unit { val callback = { resource.read() }; callback() }",
    ] {
        let observed = assert_returns(text, &[("resource.read()", "resource")]);
        assert_parameter_unowned(&observed);
    }
}

#[test]
fn rejected_later_source_revokes_nonempty_parameter_reborrow_events() {
    let prefix = format!("{NODE}fun run(node: Node): Unit {{ node.touch(\"ready\", 0) }}\n");
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
