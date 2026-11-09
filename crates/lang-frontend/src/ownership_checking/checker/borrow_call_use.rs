//! 普通结果交付 Borrow operand：明确 source handoff 与 outer CallReturn。
use super::{Checker, Flows, OwnershipCheckingError, loan::ActiveLoanOwner};
use crate::{
    ast::ExpressionId,
    diagnostic::codes,
    ownership_checking::{BorrowCallUseFact, BorrowSourceLoan, DropPoint, LoanKind, LoanTarget},
    type_checking::{
        CallableResultSource, CallableTarget, Copyability, ParameterMode, TypeId, TypeKind,
    },
};

impl Checker<'_> {
    pub(super) fn borrow_call_argument(
        &self,
        call: ExpressionId,
        argument: ExpressionId,
        parameter: TypeId,
    ) -> Option<(ExpressionId, bool)> {
        let expression = self.borrow_result_call(argument)?;
        let actual = self.typed.expression_type(argument)?;
        let copies = actual != parameter
            && matches!(
                self.typed.types().get(parameter),
                Some(TypeKind::Nullable(_))
            );
        if copies
            && !(self.typed.copyability(actual) == Some(Copyability::Copyable)
                && matches!(self.typed.types().get(parameter), Some(TypeKind::Nullable(inner)) if *inner == actual))
        {
            return None;
        }
        // A nullable conversion cannot be handed off as a stable source loan.
        let source = self.borrowed_call_source(expression)?;
        let outer = self.typed.call(call)?;
        if outer.result_source() != CallableResultSource::Owned
            || outer.receiver().is_some()
            || !matches!(outer.target(), CallableTarget::Source(_))
            || !super::super::borrow_result::call_use_is_straight_line(self.parsed, call)
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
                    && self.is_nothing_expression(expression)
            })
        {
            return None;
        }
        let inner = self.typed.call(expression)?;
        if inner.borrow_return().is_none()
            || !matches!(inner.target(), CallableTarget::Source(_))
            || inner.receiver().is_some()
            || inner
                .arguments()
                .iter()
                .any(|argument| argument.mode() == ParameterMode::Inout)
        {
            return None;
        }
        // 暂不延长 temporary source，也不借此放行嵌套结果再次成为来源。
        let place = self.place(source).ok()??;
        self.borrow_call_source_supported(&place)
            .then_some((expression, copies))
    }

    fn borrow_call_source_supported(
        &self,
        place: &crate::ownership_checking::OwnershipPlace,
    ) -> bool {
        self.typed.parameter_mode(place.root()) != Some(ParameterMode::Inout)
            && !self
                .names
                .symbols()
                .get(place.root().index())
                .is_some_and(|symbol| symbol.kind() == crate::name_resolution::SymbolKind::Field)
    }

    pub(super) fn record_borrow_call_use(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        expression: ExpressionId,
        copies: bool,
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
                loan.call() == expression
                    && loan.argument() == argument
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
        let begin = source_loan.begin_span();
        let source_loan = BorrowSourceLoan {
            call: expression,
            argument: source_loan.argument(),
        };
        let mut transferred = false;
        state.loans.retain_mut(|loan| {
            if loan.owner != ActiveLoanOwner::Call(expression) || loan.origin != begin {
                return true;
            }
            transferred = true;
            if copies {
                return false;
            }
            loan.owner = ActiveLoanOwner::Call(call);
            true
        });
        if !transferred {
            return self.emit_borrow_binding_diagnostic(
                codes::UNSUPPORTED_BORROW_FLOW,
                "borrow argument source handoff is not proven",
                self.parsed.ast().expressions().get(expression)?.span(),
            );
        }
        self.borrow_results.call_uses.push(BorrowCallUseFact {
            expression,
            call,
            argument,
            source_loan,
            origin: LoanTarget::Place(origin),
            parent,
            end_point: if copies {
                DropPoint::AfterExpression(argument)
            } else {
                DropPoint::CallReturn(call)
            },
        });
        Ok(())
    }

    pub(super) fn borrow_call_use(
        &self,
        call: ExpressionId,
        argument: ExpressionId,
    ) -> Option<
        &BorrowCallUseFact<ExpressionId, crate::name_resolution::SymbolId, LoanTarget, DropPoint>,
    > {
        self.borrow_results
            .call_uses
            .iter()
            .find(|fact| fact.call() == call && fact.argument() == argument)
    }
}
