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
