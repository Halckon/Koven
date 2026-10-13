//! 普通结果交付 Borrow operand：明确 source handoff 与 outer CallReturn。
use super::{ActiveLoanOwner, Checker, Flows, OwnershipCheckingError};
use crate::{
    ast::ExpressionId,
    diagnostic::codes,
    ownership_checking::{
        BorrowCallUseFact, BorrowSourceLoan, LoanKind, UnitDropPoint as DropPoint,
        UnitLoanTarget as LoanTarget,
    },
    type_checking::{CallableResultSource, ParameterMode, UnitCallTarget, UnitExpressionId},
};

impl Checker<'_> {
    pub(super) fn borrow_call_argument(
        &self,
        call: ExpressionId,
        argument: ExpressionId,
    ) -> Option<ExpressionId> {
        let expression = self.borrow_result_call(argument)?;
        let outer = self.typed.call(self.unit_expression(call))?;
        if outer.result_source() != CallableResultSource::Owned
            || outer.receiver().is_some()
            || !matches!(
                outer.target(),
                UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
            )
            || !crate::ownership_checking::borrow_result::call_use_is_straight_line(
                self.parsed,
                call,
            )
        {
            return None;
        }
        let span = self.parsed.ast().expressions().get(call).ok()?.span();
        if self
            .parsed
            .ast()
            .expressions()
            .iter()
            .any(|(expression, node)| {
                span.start() <= node.span().start()
                    && node.span().end() <= span.end()
                    && self.is_nothing_expression(self.unit_expression(expression))
            })
        {
            return None;
        }
        let inner = self.typed.call(self.unit_expression(expression))?;
        if inner.borrow_return().is_none()
            || !matches!(
                inner.target(),
                UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
            )
            || inner.receiver().is_some()
            || inner
                .arguments()
                .iter()
                .any(|argument| argument.mode() == ParameterMode::Inout)
        {
            return None;
        }
        // 暂不延长 temporary source，也不借此放行嵌套结果再次成为来源。
        let place = self.place(self.borrowed_call_source(expression)?).ok()??;
        self.borrow_call_source_supported(&place)
            .then_some(expression)
    }

    fn borrow_call_source_supported(
        &self,
        place: &crate::ownership_checking::UnitOwnershipPlace,
    ) -> bool {
        !self
            .bindings
            .get(&place.root())
            .is_some_and(|binding| binding.kind() == super::OwnershipBindingKind::Exclusive)
            && self.symbol_kind(place.root()) != Some(crate::name_resolution::SymbolKind::Field)
    }

    pub(super) fn record_borrow_call_use(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        expression: ExpressionId,
        flows: &mut Flows,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(state) = flows.next.as_mut() else {
            return Ok(());
        };
        let source = self.borrowed_call_source(expression);
        let place = source
            .map(|source| self.place(source))
            .transpose()?
            .flatten();
        let source_loan = source.and_then(|argument| {
            self.loans.iter().find(|loan| {
                loan.call() == self.unit_expression(expression)
                    && loan.argument() == self.unit_expression(argument)
                    && loan.kind() == LoanKind::Shared
            })
        });
        let Some((place, source_loan)) = place.zip(source_loan) else {
            return self.emit_borrow_binding_diagnostic(
                codes::UNSUPPORTED_BORROW_FLOW,
                "borrow argument has no proven stable source loan",
                self.parsed.ast().expressions().get(expression)?.span(),
            );
        };
        let origin = self.canonical_borrow_place(&place, state);
        if !self.borrow_call_source_supported(&origin) {
            return self.emit_borrow_binding_diagnostic(
                codes::UNSUPPORTED_BORROW_FLOW,
                "borrow argument receiver or inout source continuation is not yet proven",
                self.parsed.ast().expressions().get(expression)?.span(),
            );
        }
        let parent = state
            .borrow_bindings
            .contains_key(&place.root())
            .then_some(place.root());
        let source_loan = BorrowSourceLoan {
            call: self.unit_expression(expression),
            argument: source_loan.argument(),
        };
        let begin = self
            .loans
            .iter()
            .find(|loan| loan.call() == source_loan.call && loan.argument() == source_loan.argument)
            .map(|loan| loan.begin_span());
        let mut transferred = false;
        for loan in &mut state.loans {
            if loan.owner == ActiveLoanOwner::Call(self.unit_expression(expression))
                && Some(loan.origin) == begin
            {
                loan.owner = ActiveLoanOwner::Call(self.unit_expression(call));
                transferred = true;
            }
        }
        if !transferred {
            return self.emit_borrow_binding_diagnostic(
                codes::UNSUPPORTED_BORROW_FLOW,
                "borrow argument source handoff is not proven",
                self.parsed.ast().expressions().get(expression)?.span(),
            );
        }
        self.borrow_results.call_uses.push(BorrowCallUseFact {
            expression: self.unit_expression(expression),
            call: self.unit_expression(call),
            argument: self.unit_expression(argument),
            source_loan,
            origin: LoanTarget::Place(origin),
            parent,
            end_point: DropPoint::CallReturn(self.unit_expression(call)),
        });
        Ok(())
    }

    pub(super) fn borrow_call_use(
        &self,
        call: ExpressionId,
        argument: ExpressionId,
    ) -> Option<
        &BorrowCallUseFact<
            UnitExpressionId,
            crate::name_resolution::UnitSymbolId,
            LoanTarget,
            DropPoint,
        >,
    > {
        self.borrow_results.call_uses.iter().find(|fact| {
            fact.call() == self.unit_expression(call)
                && fact.argument() == self.unit_expression(argument)
        })
    }
}
