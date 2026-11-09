use super::{Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::ownership_checking::{
    LoanTarget,
    borrow_result::{
        BorrowReturnOriginFact, ReturnSource, borrow_return_source, origin_expression,
    },
};
use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity, codes},
    parser::{FunctionForm, ValueParameter},
    type_checking::{BorrowReturnOrigin, CallableResultSource, TypeId, TypeKind},
};

impl Checker<'_> {
    pub(super) fn borrow_return_source(
        &self,
        form: FunctionForm,
        parameters: &[ValueParameter],
        receiver: Option<crate::name_resolution::SymbolId>,
    ) -> Option<ReturnSource<crate::name_resolution::SymbolId>> {
        borrow_return_source(self.sources, form, parameters, receiver, |marker| {
            self.marker_symbol(marker)
        })
    }

    pub(super) fn borrowed_call_source(&self, expression: ExpressionId) -> Option<ExpressionId> {
        let (operand, parameter) = self.borrowed_call_source_operand(expression)?;
        parameter
            .is_none_or(|expected| !self.borrow_needs_nullable_conversion(operand, expected))
            .then_some(operand)
    }

    // Keep recognition of a borrow call separate from admission of its source as a place.
    pub(super) fn borrowed_call_source_operand(
        &self,
        expression: ExpressionId,
    ) -> Option<(ExpressionId, Option<TypeId>)> {
        if let Some(descriptor) = self.typed.map_require_value(expression) {
            return Some((descriptor.receiver(), None));
        }

        let call = self.call_descriptors.get(&expression.index())?;
        if let Some(range) = call.range_construction() {
            return Some((range.source(), None));
        }
        let crate::parser::Expression::Call { arguments, .. } = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .ok()?
            .payload()
        else {
            return None;
        };
        let origin = match call.result_source() {
            CallableResultSource::Borrow(contract) => contract.origin(),
            CallableResultSource::Carrier(contract) if self.range_call_is_proven(call) => {
                contract.origin()
            }
            _ => return None,
        };
        match origin {
            BorrowReturnOrigin::Parameter(index) => {
                let mapping = call
                    .arguments()
                    .iter()
                    .find(|arg| arg.parameter_index() == index)?;
                let operand = arguments.get(mapping.argument_index())?.value;
                let parameter = matches!(call.result_source(), CallableResultSource::Borrow(_))
                    .then(|| mapping.parameter_type());
                Some((operand, parameter))
            }
            BorrowReturnOrigin::Receiver
                if matches!(call.result_source(), CallableResultSource::Carrier(_)) =>
            {
                match call.receiver()?.origin() {
                    crate::type_checking::CallReceiverOrigin::Expression(id) => Some((id, None)),
                    _ => None,
                }
            }
            BorrowReturnOrigin::Receiver => None,
        }
    }

    // A narrowed read can still refer to storage already laid out as the requested nullable.
    pub(super) fn borrow_needs_nullable_conversion(
        &self,
        expression: ExpressionId,
        expected: TypeId,
    ) -> bool {
        let Some(actual) = self.typed.expression_type(expression) else {
            return false;
        };
        actual != expected
            && matches!(
                self.typed.types().get(expected),
                Some(TypeKind::Nullable(_))
            )
            && !matches!(
                self.typed.types().get(actual),
                None | Some(TypeKind::Error | TypeKind::Deferred(_))
            )
            && self
                .expression_root_symbol(expression)
                .ok()
                .flatten()
                .and_then(|symbol| self.typed.symbol_type(symbol))
                != Some(expected)
    }

    pub(super) fn check_borrow_call_use(
        &mut self,
        expression: ExpressionId,
        owned: bool,
    ) -> Result<(), OwnershipCheckingError> {
        if self.check_range_call_use(expression, owned)? {
            return Ok(());
        }
        let Some(contract) = self.typed.call_borrow_return(expression) else {
            return Ok(());
        };
        if !owned && self.allowed_borrow_call == Some(expression) {
            return Ok(());
        }
        let (code, message) = if owned {
            (
                codes::BORROW_RESULT_ESCAPE,
                "borrow result cannot be delivered to an owned destination",
            )
        } else {
            (
                codes::UNSUPPORTED_BORROW_FLOW,
                "caller borrow result continuation is not yet proven",
            )
        };
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            codes::catalog()?.resolve(code)?,
            message,
            self.parsed.ast().expressions().get(expression)?.span(),
        )?;
        diagnostic.add_label(
            self.sources,
            contract.marker_span(),
            "borrow result declared here",
        )?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    pub(super) fn check_borrow_return(
        &mut self,
        expression: ExpressionId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let Some(source) = self.current_borrow_return else {
            return self.check_escaping_expression(expression, state, ExpressionUse::Consume);
        };
        if matches!(
            self.parsed.ast().expressions().get(expression)?.payload(),
            crate::parser::Expression::If { .. } | crate::parser::Expression::When { .. }
        ) {
            self.diagnostics.push(Diagnostic::new(
                self.sources,
                Severity::Error,
                codes::catalog()?.resolve(codes::UNSUPPORTED_BORROW_FLOW)?,
                "borrow return control-flow origin is not yet proven",
                self.parsed.ast().expressions().get(expression)?.span(),
            )?);
            return Ok(Flows::next(state));
        }
        if source.new_range
            && !self
                .borrow_result_call(expression)
                .is_some_and(|id| self.range_expression_is_proven(id))
        {
            self.diagnostics.push(Diagnostic::new(
                self.sources,
                Severity::Error,
                codes::catalog()?.resolve(codes::UNSUPPORTED_BORROW_FLOW)?,
                "new range return requires an actual construction or proven producer forwarding",
                self.parsed.ast().expressions().get(expression)?.span(),
            )?);
            return Ok(Flows::next(state));
        }
        let before = self.diagnostics.len();
        let previous = std::mem::replace(&mut self.checking_borrow_return, true);
        let call = self.borrow_result_call(expression);
        let allowed = std::mem::replace(&mut self.allowed_borrow_call, call);
        let checked = self.check_expression(expression, state, ExpressionUse::Read);
        self.checking_borrow_return = previous;
        self.allowed_borrow_call = allowed;
        let flows = checked?;
        if flows.next.is_none() {
            return Ok(flows);
        }
        let Some(symbol) = source.symbol else {
            self.diagnostics.push(Diagnostic::new(
                self.sources,
                Severity::Error,
                codes::catalog()?.resolve(codes::UNSUPPORTED_BORROW_FLOW)?,
                "borrow result source requires an unsupported receiver or inout continuation",
                source.marker,
            )?);
            return Ok(flows);
        };
        let origin = origin_expression(self.parsed, expression, |call| {
            self.borrowed_call_source(call)
        })?;
        let place = origin
            .map(|origin| self.place(origin))
            .transpose()?
            .flatten();
        let needs_conversion = self
            .typed
            .type_ref_type(source.type_ref)
            .is_some_and(|expected| self.borrow_needs_nullable_conversion(expression, expected));
        if let Some(place) = place.filter(|place| place.root() == symbol && !needs_conversion) {
            if self.diagnostics.len() == before {
                if let Some(call) = self.borrow_result_call(expression)
                    && let Some(argument) = self.borrowed_call_source(call)
                {
                    self.borrow_results
                        .forwarded
                        .push(crate::ownership_checking::BorrowSourceLoan { call, argument });
                }
                if source.new_range {
                    self.borrow_results.range_returns.push(
                        crate::ownership_checking::RangeReturnOriginFact::new(
                            expression,
                            LoanTarget::Place(place),
                            source.marker,
                        ),
                    );
                } else {
                    self.borrow_return_origins.push(BorrowReturnOriginFact::new(
                        expression,
                        LoanTarget::Place(place),
                        source.marker,
                    ));
                }
            }
        } else {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                codes::catalog()?.resolve(codes::INVALID_BORROW_CONTRACT)?,
                if needs_conversion {
                    "returned borrow requires existing nullable storage"
                } else {
                    "returned borrow does not originate from the declared source"
                },
                self.parsed.ast().expressions().get(expression)?.span(),
            )?;
            diagnostic.add_label(
                self.sources,
                source.span,
                "required borrow source declared here",
            )?;
            self.diagnostics.push(diagnostic);
        }
        Ok(flows)
    }
}
