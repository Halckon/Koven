//! Subset admission for the temporary that an unfinished call already borrows.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{
        ConstantMaterializationKind, LoanKind, UnitDropFact, UnitLoanFact, UnitLoanTarget,
    },
    parser::Expression,
    type_checking::{ParameterMode, UnitCallReceiverOrigin, UnitExpressionId},
};

use super::Checker;
use crate::ssa::refine::SubsetDropIndex;

impl Checker<'_, '_> {
    /// Borrow returns can unwind a call when they preserve the incoming String loan.
    pub(in crate::ssa::refine::subset) fn pending_call_loans_end_on(
        &self,
        transfer: ExpressionId,
    ) -> bool {
        if self.inputs.carrier_return.is_some() {
            return false;
        }
        if self.inputs.borrow_return.is_none() {
            return true;
        }
        let Ok(node) = self.inputs.parsed.ast().expressions().get(transfer) else {
            return false;
        };
        match node.payload() {
            Expression::Return { .. } => self.returns_from_string_entry(transfer),
            Expression::Break { .. } | Expression::Continue { .. } => {
                self.inputs.string_entry_source().is_some()
            }
            _ => false,
        }
    }

    /// Ordinary control exits end pending call loans before emitting any drops.
    pub(in crate::ssa::refine::subset) fn pending_direct_borrow_temporary(
        &self,
        owner: UnitExpressionId,
        transfer: UnitExpressionId,
        drop: &UnitDropFact,
        loan: &UnitLoanFact,
    ) -> bool {
        let call = loan.call();
        let argument = loan.argument();
        // A grouped constant borrows the inner materialized String owner.
        let origin = if argument == owner {
            Some(loan.begin_span())
        } else if argument.source_unit() == self.inputs.source_unit
            && owner.source_unit() == self.inputs.source_unit
            && self.ungrouped(argument.expression()) == owner.expression()
            && self.inputs.constant_owned.is_some_and(|owned| {
                owned
                    .materialization_at(owner)
                    .is_some_and(|plan| plan.kind() == ConstantMaterializationKind::StringTemporary)
            })
        {
            self.inputs
                .parsed
                .ast()
                .expressions()
                .get(owner.expression())
                .ok()
                .map(|node| node.span())
        } else {
            None
        };
        transfer.source_unit() == self.inputs.source_unit
            && loan.kind() == LoanKind::Shared
            && loan.target() == &UnitLoanTarget::Temporary(owner)
            && !drop.is_conditional_temporary()
            && Some(drop.value_origin()) == origin
            && self.inputs.owned.call_argument_completed(argument)
            && self
                .inputs
                .source_plan
                .call_site(self.inputs.source_token, call)
                .is_some_and(|site| {
                    site.delegation().is_empty()
                        && self.inputs.owned.call_may_dispatch(call)
                            == Some(site.dispatch_key().is_some())
                })
            && (self.inputs.borrow_return.is_none()
                || self.inputs.typed.call(call).is_some_and(|descriptor| {
                    descriptor.receiver().is_some_and(|receiver| {
                        receiver.mode() == ParameterMode::Borrow
                            && matches!(
                                receiver.origin(),
                                UnitCallReceiverOrigin::Expression(receiver)
                                    if receiver.source_unit() == self.inputs.source_unit
                                        && self.object_name(receiver.expression())
                            )
                    })
                }))
            && self.pending_call_loans_end_on(transfer.expression())
            && SubsetDropIndex::ends_before_drops(
                self.inputs.subset_drop_index,
                [
                    self.inputs.owned.direct_call_loan_ends(),
                    self.inputs.owned.class_result_argument_unwinds(),
                    self.inputs.owned.singleton_temporary_argument_unwinds(),
                ],
                transfer,
                loan,
            )
    }
}
