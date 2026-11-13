//! CI-only mutations of actual parent SSA; never called by production builds.
use super::*;
impl Emitted {
    pub(in super::super) fn inject_parent(
        &mut self,
        body: &super::super::body::RefinedBody,
        function: &mut Function,
    ) {
        use super::super::build::owner_consumer_test_support::{
            OwnerFault, record_action, take_fault,
        };
        let Some(p) = &body.nested_exit_plans.parent else {
            return;
        };
        let header = self.parent.blocks[&p.header];
        let exit = self.parent.blocks[&p.exit];
        let mut changed = false;
        if take_fault(OwnerFault::ParentFalseSwapArguments) {
            let positions = p
                .roots
                .iter()
                .map(|r| {
                    self.parent.slots[&p.exit]
                        .iter()
                        .position(|s| s == r)
                        .unwrap()
                })
                .collect::<Vec<_>>();
            let term = &mut function
                .blocks
                .iter_mut()
                .find(|b| b.id == header)
                .unwrap()
                .terminator
                .as_mut()
                .unwrap()
                .kind;
            let TerminatorKind::Conditional { when_false, .. } = term else {
                panic!("parent header");
            };
            when_false.arguments.swap(positions[0], positions[1]);
            changed = true;
        }
        if take_fault(OwnerFault::ParentFalseExtraPredecessor) {
            let args = match &function
                .block(header)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .kind
            {
                TerminatorKind::Conditional { when_false, .. } => when_false.arguments.clone(),
                _ => panic!("parent header"),
            };
            let other = function
                .blocks
                .iter_mut()
                .find(|b| b.id != header && b.id != exit)
                .unwrap();
            other.terminator.as_mut().unwrap().kind =
                TerminatorKind::Branch(crate::ssa::model::Edge {
                    target: exit,
                    arguments: args,
                });
            changed = true;
        }
        if let Some((_, (id, EntityId::Value(owner)))) = self.parent.observed.first_key_value() {
            if take_fault(OwnerFault::ParentFalseMissingDrop) {
                function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == exit)
                    .unwrap()
                    .instructions
                    .retain(|i| i != id);
                changed = true;
            }
            if take_fault(OwnerFault::ParentFalseSecondDrop) {
                let origin = function.instruction(*id).unwrap().origin.clone();
                let saved = function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == exit)
                    .unwrap()
                    .terminator
                    .take();
                function
                    .append_instruction(exit, Operation::Drop { owner: *owner }, Vec::new(), origin)
                    .unwrap();
                function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == exit)
                    .unwrap()
                    .terminator = saved;
                changed = true;
            }
            if take_fault(OwnerFault::ParentFalseEscape) {
                function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == exit)
                    .unwrap()
                    .terminator
                    .as_mut()
                    .unwrap()
                    .kind = TerminatorKind::Return {
                    values: vec![*owner],
                };
                changed = true;
            }
        }
        // Lose real recorded state after layout/observation, leaving valid SSA intact.
        // The independent pair receipt must reject without help from the verifier.
        let lost_receipt = take_fault(OwnerFault::ParentFalseMissingReceipt);
        let lost_observation = take_fault(OwnerFault::ParentFalseMissingObservation);
        if lost_receipt || lost_observation {
            if lost_receipt {
                self.parent = Receipt::default();
            } else {
                assert!(!self.parent.observed.is_empty());
                self.parent.observed.pop_first();
            }
            record_action("parent-false-receipt-fault-hit", 1);
            let span = function
                .block(header)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .origin
                .span();
            assert!(
                self.finish(body, function, span).is_err(),
                "missing parent receipt event must fail with valid SSA"
            );
        }
        if changed {
            record_action("parent-false-ssa-fault-hit", 1);
            let span = function
                .block(header)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .origin
                .span();
            assert!(
                self.parent
                    .finish(&body.nested_exit_plans.parent, function, span)
                    .is_err(),
                "parent receipt must reject before generic verifier"
            );
        }
    }
}
