//! 只投影实际 Return unwind 的 receiver token，不重建或改变 cleanup。
use super::{DropPlanner, PlannerDropPoint};
use crate::{
    ownership_checking::{
        UnitIterationCleanupAction as Action, UnitReceiverCallPrefixEndFact,
        UnitReceiverOwnershipFact, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
    },
    parser::Expression,
    type_checking::{CallableResultSource, UnitCallReceiverOrigin, UnitCallTarget},
};
use std::collections::BTreeMap;

impl DropPlanner<'_, '_> {
    fn observes_receiver_prefix(&self, receiver: &UnitReceiverOwnershipFact) -> bool {
        let Some(call) = self.checker.typed.call(receiver.call()) else {
            return false;
        };
        if receiver.kind() != UnitReceiverOwnershipKind::SharedLoan
            || !matches!(
                call.target(),
                UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
            )
            || call.result_source() != CallableResultSource::Owned
        {
            return false;
        }
        let UnitCallReceiverOrigin::Expression(operand) = receiver.source() else {
            return false;
        };
        let mut operand = operand.expression();
        loop {
            let Ok(node) = self.checker.parsed.ast().expressions().get(operand) else {
                return false;
            };
            match node.payload() {
                Expression::Group { expression } => operand = *expression,
                Expression::Name => break,
                _ => return false,
            }
        }
        let Ok(Some(place)) = self.checker.place(operand) else {
            return false;
        };
        place.is_root()
            && self.is_observable_owned_loan_root(place.root())
            && matches!(receiver.target(), UnitReceiverOwnershipTarget::Place(target) if target == &place)
    }

    pub(super) fn receiver_call_prefix_ends(&self) -> Vec<UnitReceiverCallPrefixEndFact> {
        let mut counts = BTreeMap::<_, (usize, usize, usize)>::new();
        let mut ends = Vec::new();
        for (point, action) in &self.iteration_actions {
            let PlannerDropPoint::ControlTransfer(transfer) = point else {
                continue;
            };
            if !self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(*transfer)
                .is_ok_and(|node| matches!(node.payload(), Expression::Return { .. }))
            {
                continue;
            }
            let (drops, calls, receivers) = counts.entry(transfer.index()).or_default();
            match action {
                Action::Drop(_) => *drops += 1,
                Action::EndCallLoan(_) => *calls += 1,
                Action::EndReceiverLoan(receiver) => {
                    if self.observes_receiver_prefix(receiver) {
                        ends.push(UnitReceiverCallPrefixEndFact {
                            receiver: receiver.clone(),
                            point: point.into_unit(self.checker.source_unit),
                            preceding_drops: *drops,
                            preceding_call_loan_ends: *calls,
                            preceding_receiver_loan_ends: *receivers,
                        });
                    }
                    *receivers += 1;
                }
                _ => {}
            }
        }
        ends
    }
}
