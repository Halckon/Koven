//! 观察正常完成及 return/break/continue 前缀中真实退休的 receiver token；不重建或改变 cleanup。
use super::{DropPlanner, PlannerDropPoint, ValueState};
use crate::{
    ast::ExpressionId,
    name_resolution::SymbolKind,
    ownership_checking::{
        BorrowBindingStorage, OwnershipBindingKind, UnitIterationCleanupAction as Action,
        UnitLoanTarget, UnitReceiverCallPrefixEndFact, UnitReceiverCallReturnEndFact,
        UnitReceiverOwnershipFact, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
    },
    parser::Expression,
    type_checking::{CallableResultSource, UnitCallReceiverOrigin, UnitCallTarget, UnitTypeKind},
};
use std::collections::BTreeMap;

impl DropPlanner<'_, '_> {
    fn has_named_receiver_place_syntax(&self, mut operand: ExpressionId) -> bool {
        loop {
            let Ok(node) = self.checker.parsed.ast().expressions().get(operand) else {
                return false;
            };
            match node.payload() {
                Expression::Group { expression } => operand = *expression,
                Expression::Member {
                    receiver,
                    safe: false,
                    ..
                } => operand = *receiver,
                Expression::Name => return true,
                _ => return false,
            }
        }
    }

    fn observes_bounded_receiver(&self, receiver: &UnitReceiverOwnershipFact) -> bool {
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
        // This uses the already checked nominal identity, not the planner's
        // current receiver context (prefix observations are collected later).
        if let UnitReceiverOwnershipTarget::This(owner) = receiver.target() {
            let concrete = matches!(
                self.checker.typed.types().get(receiver.receiver_type()),
                Some(UnitTypeKind::Nominal { declaration, .. }) if declaration == owner
            );
            let same_source = match receiver.source() {
                UnitCallReceiverOrigin::ImplicitThis(source) => source == *owner,
                UnitCallReceiverOrigin::Expression(source) => {
                    source.source_unit() == receiver.call().source_unit()
                        && self
                            .checker
                            .expression_is_this(source.expression())
                            .unwrap_or(false)
                }
            };
            let provider = self.checker.iterations.values().any(|plan| {
                matches!(plan.source(), Some(UnitLoanTarget::This(source)) if source == owner)
            }) || self.checker.map_iterations.values().any(|plan| {
                matches!(plan.source(), Some(UnitLoanTarget::This(source)) if source == owner)
            });
            return concrete && same_source && !provider;
        }
        let UnitCallReceiverOrigin::Expression(operand) = receiver.source() else {
            return false;
        };
        let full_operand = operand.expression();
        if !self.has_named_receiver_place_syntax(full_operand) {
            return false;
        }
        // Resolve the complete typed field path, not merely its named backing root.
        // The syntax walk excludes this fields, indices, safe access and temporaries.
        let Ok(Some(operand_place)) = self.checker.place(full_operand) else {
            return false;
        };
        // Direct first-level bindings retain a canonical source lease. Observe
        // the existing child token; never reinterpret the alias as an owned root
        // or infer a payload address from a callee's borrow-result source.
        let place = if let Some(binding) = self
            .checker
            .borrow_results
            .bindings
            .iter()
            .find(|binding| binding.binding() == operand_place.root())
        {
            if !operand_place.is_root()
                || binding.storage() != BorrowBindingStorage::BorrowedStorage
                || binding.parent().is_some()
                || binding.source_loan().is_some()
                || binding.initializer().source_unit() != receiver.call().source_unit()
                || !self.has_named_receiver_place_syntax(binding.initializer().expression())
            {
                return false;
            }
            let UnitLoanTarget::Place(origin) = binding.origin() else {
                return false;
            };
            let Ok(Some(initializer)) = self.checker.place(binding.initializer().expression())
            else {
                return false;
            };
            if &initializer != origin {
                return false;
            }
            origin
        } else {
            &operand_place
        };
        let root = place.root();
        let owned = self.checker.formed_owned_locals.contains(&root)
            || self
                .checker
                .bindings
                .get(&root)
                .is_some_and(|binding| binding.kind() == OwnershipBindingKind::Owned);
        // A nested Shared receiver reborrow may end while the caller's parameter
        // loan and owner remain live. Borrow/Inout parameters may both establish
        // Shared reborrows; this does not admit an Exclusive receiver reservation.
        let nonowning_parameter = self.checker.symbol_kind(root)
            == Some(SymbolKind::ValueParameter)
            && self.checker.bindings.get(&root).is_some_and(|binding| {
                matches!(
                    binding.kind(),
                    OwnershipBindingKind::Shared | OwnershipBindingKind::Exclusive
                )
            });
        // A root descriptor alone cannot distinguish provider/binding provenance.
        let provider = self.checker.iterations.values().any(|plan| {
            plan.bindings().iter().any(|binding| binding.symbol() == root)
                || matches!(plan.source(), Some(UnitLoanTarget::Place(source)) if source.root() == root)
        }) || self.checker.map_iterations.values().any(|plan| {
            plan.bindings().iter().any(|binding| binding.symbol() == root)
                || matches!(plan.source(), Some(UnitLoanTarget::Place(source)) if source.root() == root)
        });
        (owned || nonowning_parameter)
            && !provider
            && place.elements().is_empty()
            && matches!(
                self.checker.symbol_kind(root),
                Some(SymbolKind::Variable | SymbolKind::ValueParameter)
            )
            && !self
                .checker
                .borrow_results
                .bindings
                .iter()
                .any(|binding| binding.binding() == root)
            && matches!(receiver.target(), UnitReceiverOwnershipTarget::Place(target) if target == place)
    }

    /// Retire exactly the tokens removed by the old normal-return retain. Move the
    /// qualifying receiver token into its own observation, never synthesize it from
    /// final descriptors. This does not add iteration actions or claim cross-table order.
    pub(super) fn finish_call_pending_loans(&mut self, call: ExpressionId, state: &mut ValueState) {
        let call = self.checker.unit_expression(call);
        for (action, _) in state
            .pending_loans
            .extract_if(.., |(action, _)| match action {
                Action::EndCallLoan(loan) => loan.call() == call,
                Action::EndReceiverLoan(receiver) => receiver.call() == call,
                _ => false,
            })
        {
            if let Action::EndReceiverLoan(receiver) = action
                && self.observes_bounded_receiver(&receiver)
            {
                self.receiver_call_return_ends
                    .push(UnitReceiverCallReturnEndFact { receiver });
            }
        }
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
                .is_ok_and(|node| {
                    matches!(
                        node.payload(),
                        Expression::Return { .. }
                            | Expression::Break { .. }
                            | Expression::Continue { .. }
                    )
                })
            {
                continue;
            }
            let (drops, calls, receivers) = counts.entry(transfer.index()).or_default();
            match action {
                Action::Drop(_) => *drops += 1,
                Action::EndCallLoan(_) => *calls += 1,
                Action::EndReceiverLoan(receiver) => {
                    if self.observes_bounded_receiver(receiver) {
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
