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
