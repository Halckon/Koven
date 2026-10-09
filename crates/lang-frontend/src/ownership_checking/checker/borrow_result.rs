use super::{Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::ownership_checking::{
    LoanTarget,
    borrow_result::{BorrowReturnOriginFact, ReturnSource, marker_span, origin_expression},
};
use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity, codes},
    parser::{BorrowReturnSource, FunctionForm, NameMarker, ParameterModeMarker, ValueParameter},
};

impl Checker<'_> {
    pub(super) fn borrow_return_source(
        &self,
        form: FunctionForm,
        parameters: &[ValueParameter],
    ) -> Option<ReturnSource<crate::name_resolution::SymbolId>> {
        let FunctionForm::Explicit {
            borrow_return: Some(syntax),
            ..
        } = form
        else {
            return None;
        };
        let (symbol, span) = match syntax.source {
            BorrowReturnSource::Parameter(NameMarker::Present(span)) => {
                let parameter = parameters.iter().find(|parameter| {
                    self.sources.slice(marker_span(parameter.name)).ok()
                        == self.sources.slice(span).ok()
                });
                let symbol = parameter
                    .filter(|p| {
                        !matches!(
                            p.mode_marker,
                            Some(ParameterModeMarker::Inout(_) | ParameterModeMarker::Own(_))
                        )
                    })
                    .and_then(|p| self.marker_symbol(p.name));
                (symbol, parameter.map_or(span, |p| marker_span(p.name)))
            }
            BorrowReturnSource::Parameter(NameMarker::Missing(span) | NameMarker::Error(span))
            | BorrowReturnSource::Receiver(span) => (None, span),
        };
        Some(ReturnSource {
            symbol,
            span,
            marker: syntax.borrow_span,
        })
    }

    pub(super) fn check_borrow_call_use(
        &mut self,
        expression: ExpressionId,
        owned: bool,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(contract) = self
            .typed
            .call(expression)
            .and_then(|call| call.borrow_return())
        else {
            return Ok(());
        };
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
        let mut value = expression;
        while let crate::parser::Expression::Group { expression: inner } =
            self.parsed.ast().expressions().get(value)?.payload()
        {
            value = *inner;
        }
        if matches!(
            self.parsed.ast().expressions().get(value)?.payload(),
            crate::parser::Expression::If { .. }
                | crate::parser::Expression::When { .. }
                | crate::parser::Expression::Call { .. }
        ) {
            self.diagnostics.push(Diagnostic::new(
                self.sources,
                Severity::Error,
                codes::catalog()?.resolve(codes::UNSUPPORTED_BORROW_FLOW)?,
                "borrow return control-flow or call continuation is not yet proven",
                self.parsed.ast().expressions().get(expression)?.span(),
            )?);
            return Ok(Flows::next(state));
        }
        let before = self.diagnostics.len();
        let flows = self.check_expression(expression, state, ExpressionUse::Read)?;
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
        let origin = origin_expression(self.parsed, expression)?;
        let place = origin
            .map(|origin| self.place(origin))
            .transpose()?
            .flatten();
        if let Some(place) = place.filter(|place| place.root() == symbol) {
            if self.diagnostics.len() == before {
                self.borrow_return_origins.push(BorrowReturnOriginFact::new(
                    expression,
                    LoanTarget::Place(place),
                    source.marker,
                ));
            }
        } else {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                codes::catalog()?.resolve(codes::INVALID_BORROW_CONTRACT)?,
                "returned borrow does not originate from the declared source",
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
