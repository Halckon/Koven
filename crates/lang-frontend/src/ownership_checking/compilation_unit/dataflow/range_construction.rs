//! source-qualified 新范围 metadata 的稳定根交付；不把签名当作 callee 返回证明。
use super::*;
use crate::ownership_checking::BorrowBindingStorage;
use crate::type_checking::{CallableResultSource, IntrinsicTypeConstructor, UnitTypeKind};

impl Checker<'_> {
    pub(super) fn record_range_use(
        &mut self,
        expression: ExpressionId,
        site: crate::ownership_checking::RangeUseSite<crate::type_checking::UnitExpressionId>,
    ) {
        let Some(argument) = self.borrowed_call_source(expression) else {
            return;
        };
        let call = self.unit_expression(expression);
        let argument = self.unit_expression(argument);
        let Some(loan) = self.loans.iter().find(|loan| {
            loan.call() == call
                && loan.argument() == argument
                && loan.kind() == crate::ownership_checking::LoanKind::Shared
        }) else {
            return;
        };
        self.borrow_results
            .range_uses
            .push(crate::ownership_checking::RangeUseFact {
                expression: call,
                source_loan: crate::ownership_checking::BorrowSourceLoan { call, argument },
                origin: loan.target().clone(),
                site,
            });
    }
    // The producer's source loan survives until its actual consumer ends.
    pub(super) fn continue_range_source(
        &self,
        expression: ExpressionId,
        owner: super::ActiveLoanOwner,
        flows: &mut Flows,
    ) {
        let call = self.unit_expression(expression);
        if let Some(fact) = self.range_use(expression) {
            let source = fact.source_loan();
            let begin = self
                .loans
                .iter()
                .find(|loan| loan.call() == source.call() && loan.argument() == source.argument())
                .map(|loan| loan.begin_span());
            for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
                .into_iter()
                .flatten()
            {
                for loan in &mut state.loans {
                    if loan.owner == super::ActiveLoanOwner::Call(call)
                        && Some(loan.origin) == begin
                    {
                        loan.owner = owner;
                    }
                }
            }
        }
    }
    pub(super) fn range_use(
        &self,
        expression: ExpressionId,
    ) -> Option<
        &crate::ownership_checking::RangeUseFact<
            crate::type_checking::UnitExpressionId,
            crate::ownership_checking::UnitLoanTarget,
        >,
    > {
        self.borrow_results
            .range_uses
            .iter()
            .find(|fact| fact.expression() == self.unit_expression(expression))
    }
    pub(super) fn is_range_return_expression(&self, expression: ExpressionId) -> bool {
        self.borrow_results
            .range_returns
            .iter()
            .any(|fact| fact.expression() == self.unit_expression(expression))
    }
    pub(super) fn range_call_is_proven(
        &self,
        call: &crate::type_checking::UnitCallDescriptor,
    ) -> bool {
        if call.range_construction().is_some() {
            return true;
        }
        if !matches!(call.result_source(), CallableResultSource::Carrier(_)) {
            return false;
        }
        let Some(signature) =
            crate::ownership_checking::compilation_unit::contracts::source_callable_signature(
                self.typed,
                call.target(),
            )
        else {
            return false;
        };
        self.range_producers.contains(&signature.name_span())
    }
    pub(super) fn range_expression_is_proven(&self, expression: ExpressionId) -> bool {
        self.typed
            .calls()
            .iter()
            .find(|call| call.expression() == self.unit_expression(expression))
            .is_some_and(|call| self.range_call_is_proven(call))
    }

    pub(super) fn range_binding_storage(
        &self,
        initializer: ExpressionId,
        call: Option<ExpressionId>,
    ) -> BorrowBindingStorage {
        if call
            .and_then(|id| {
                self.typed
                    .calls()
                    .iter()
                    .find(|c| c.expression() == self.unit_expression(id))
            })
            .is_some_and(|c| self.range_call_is_proven(c))
        {
            return BorrowBindingStorage::NewRangeDescriptor;
        }
        if self
            .typed
            .expression_type(self.unit_expression(initializer))
            .is_some_and(|ty| {
                matches!(
                    self.typed.types().get(ty),
                    Some(UnitTypeKind::Intrinsic {
                        constructor: IntrinsicTypeConstructor::View,
                        ..
                    })
                )
            })
        {
            return BorrowBindingStorage::BorrowedCarrierMetadata;
        }
        BorrowBindingStorage::BorrowedStorage
    }

    pub(super) fn check_range_call_use(
        &mut self,
        expression: ExpressionId,
        owned: bool,
    ) -> Result<bool, OwnershipCheckingError> {
        let Some(call) = self
            .typed
            .calls()
            .iter()
            .find(|c| c.expression() == self.unit_expression(expression))
        else {
            return Ok(false);
        };
        let CallableResultSource::Carrier(contract) = call.result_source() else {
            return Ok(false);
        };
        if self.range_call_is_proven(call) && !owned && self.allowed_borrow_call == Some(expression)
        {
            return Ok(true);
        }
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            codes::catalog()?.resolve(codes::UNSUPPORTED_BORROW_FLOW)?,
            "range delivery requires a proven producer and a stable binding or return continuation",
            self.parsed.ast().expressions().get(expression)?.span(),
        )?;
        diagnostic.add_label(
            self.sources,
            contract.marker_span(),
            "range delivery source contract",
        )?;
        self.diagnostics.push(diagnostic);
        Ok(true)
    }
}
