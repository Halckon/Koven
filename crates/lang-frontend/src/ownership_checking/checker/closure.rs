use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Diagnostic, Severity},
    name_resolution::SymbolId,
    parser::Expression,
    source::Span,
    type_checking::ParameterMode,
};

use crate::ownership_checking::{
    ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource, LoanKind, OwnershipPlace,
    Transferability,
};

use super::loan::{ActiveLoan, ActiveLoanOwner, ActiveLoanTarget};
use super::{AccessKind, Checker, Flows, OwnershipCheckingError, State, span_key};

impl Checker<'_> {
    pub(super) fn check_lambda(
        &mut self,
        lambda: ExpressionId,
        parameters: &[Span],
        body: StatementId,
        mut state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let captures = self.captures_of(lambda).collect::<Vec<_>>();
        for capture in &captures {
            match (capture.mode(), capture.source(), capture.effect()) {
                (ClosureCaptureMode::Shared, ClosureCaptureSource::Symbol(symbol), _) => {
                    let place = OwnershipPlace::new(symbol, Vec::new());
                    self.ensure_place_available(&place, capture.reference_span(), &state)?;
                    if self.access_place(
                        &place,
                        AccessKind::SharedLoan,
                        false,
                        capture.reference_span(),
                        &mut state,
                    )? {
                        state.loans.push(ActiveLoan {
                            owner: ActiveLoanOwner::Closure(lambda),
                            target: ActiveLoanTarget::Place(place),
                            kind: LoanKind::Shared,
                            origin: capture.reference_span(),
                        });
                    }
                }
                (ClosureCaptureMode::Shared, ClosureCaptureSource::This, _) => {
                    state.loans.push(ActiveLoan {
                        owner: ActiveLoanOwner::Closure(lambda),
                        target: ActiveLoanTarget::This,
                        kind: LoanKind::Shared,
                        origin: capture.reference_span(),
                    });
                }
                (ClosureCaptureMode::Owned, ClosureCaptureSource::This, _) => {
                    self.emit_illegal_owned_capture(
                        capture.reference_span(),
                        capture.reference_span(),
                        "move closure cannot capture this or a field directly",
                    )?;
                }
                (
                    ClosureCaptureMode::Owned,
                    ClosureCaptureSource::Symbol(symbol),
                    ClosureCaptureEffect::Move,
                ) => {
                    let binding = self.names.symbols()[symbol.index()].span();
                    if matches!(
                        self.typed.parameter_mode(symbol),
                        Some(ParameterMode::Borrow | ParameterMode::Inout)
                    ) || state.non_owning.contains_key(&symbol)
                    {
                        self.emit_illegal_owned_capture(
                            capture.reference_span(),
                            binding,
                            "move closure cannot own a non-Copyable borrowed capture",
                        )?;
                        continue;
                    }
                    let place = OwnershipPlace::new(symbol, Vec::new());
                    self.ensure_place_available(&place, capture.reference_span(), &state)?;
                    if self.access_place(
                        &place,
                        AccessKind::Move,
                        true,
                        capture.reference_span(),
                        &mut state,
                    )? {
                        state.moved.insert(symbol, capture.reference_span());
                    }
                }
                (
                    ClosureCaptureMode::Owned,
                    ClosureCaptureSource::Symbol(symbol),
                    ClosureCaptureEffect::Copy | ClosureCaptureEffect::Unknown,
                ) => {
                    let place = OwnershipPlace::new(symbol, Vec::new());
                    self.ensure_place_available(&place, capture.reference_span(), &state)?;
                    self.access_place(
                        &place,
                        AccessKind::Read,
                        false,
                        capture.reference_span(),
                        &mut state,
                    )?;
                }
                (
                    ClosureCaptureMode::Owned,
                    ClosureCaptureSource::Symbol(_),
                    ClosureCaptureEffect::Borrow,
                ) => unreachable!("owned captures never publish a borrow effect"),
            }
        }

        let mut body_state = State::default();
        for parameter in parameters {
            if let Some(symbol) = self.symbols_by_span.get(&span_key(*parameter)).copied() {
                body_state.moved.remove(&symbol);
            }
        }
        for capture in captures {
            let ClosureCaptureSource::Symbol(symbol) = capture.source() else {
                continue;
            };
            body_state.moved.remove(&symbol);
            body_state
                .immutable_captures
                .insert(symbol, capture.reference_span());
            if capture.mode() == ClosureCaptureMode::Shared {
                body_state
                    .non_owning
                    .insert(symbol, capture.reference_span());
            }
        }
        self.check_statement(body, body_state)?;
        Ok(Flows::next(state))
    }

    fn emit_illegal_owned_capture(
        &mut self,
        primary: Span,
        binding: Span,
        message: &'static str,
    ) -> Result<(), OwnershipCheckingError> {
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.illegal_owned_capture_code,
            message,
            primary,
        )?;
        diagnostic.add_label(self.sources, binding, "capture source is non-owning here")?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    pub(super) fn closure_origin(
        &self,
        expression: ExpressionId,
        state: &State,
    ) -> Result<Option<ExpressionId>, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::Lambda { .. } => Ok(Some(expression)),
            Expression::Group { expression } => self.closure_origin(*expression, state),
            Expression::Name => Ok(self
                .reference_symbol(node.span())
                .and_then(|symbol| state.closures.get(&symbol).copied())),
            _ => Ok(None),
        }
    }

    pub(super) fn expression_root_symbol(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<SymbolId>, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::Name => Ok(self.reference_symbol(node.span())),
            Expression::Group { expression } => self.expression_root_symbol(*expression),
            _ => Ok(None),
        }
    }

    pub(super) fn release_closure(&self, symbol: SymbolId, state: &mut State) {
        let Some(closure) = state.closures.remove(&symbol) else {
            return;
        };
        if state.closures.values().any(|&other| other == closure) {
            return;
        }
        state
            .loans
            .retain(|loan| loan.owner != ActiveLoanOwner::Closure(closure));
    }

    pub(super) fn release_last_closure_use(
        &self,
        expression: ExpressionId,
        flows: &mut Flows,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(symbol) = self.expression_root_symbol(expression)? else {
            return Ok(());
        };
        if self.expression_live_after[expression.index()].contains(&symbol) {
            return Ok(());
        }
        for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
            .into_iter()
            .flatten()
        {
            self.release_closure(symbol, state);
        }
        Ok(())
    }

    pub(super) fn reject_borrowed_closure_escape(
        &mut self,
        expression: ExpressionId,
        state: &State,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(lambda) = self.closure_origin(expression, state)? else {
            return Ok(());
        };
        if !self
            .captures_of(lambda)
            .any(|capture| capture.mode() == ClosureCaptureMode::Shared)
        {
            return Ok(());
        }
        let primary = self.parsed.ast().expressions().get(expression)?.span();
        let lambda_span = self.parsed.ast().expressions().get(lambda)?.span();
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.borrowed_closure_escape_code,
            "borrowed closure cannot escape its defining callable",
            primary,
        )?;
        diagnostic.add_label(self.sources, lambda_span, "borrowed closure formed here")?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    pub(super) fn check_cross_thread_delivery(
        &mut self,
        expression: ExpressionId,
        state: &State,
    ) -> Result<(), OwnershipCheckingError> {
        let closure = self.closure_origin(expression, state)?;
        let transferability = closure
            .and_then(|lambda| {
                self.closures
                    .iter()
                    .find(|descriptor| descriptor.expression() == lambda)
                    .map(|descriptor| descriptor.transferability())
            })
            .or_else(|| {
                self.typed
                    .expression_type(expression)
                    .and_then(|ty| self.transferabilities.get(ty.index()).copied())
            });
        if transferability == Some(Transferability::Transferable) {
            return Ok(());
        }
        let primary = self.parsed.ast().expressions().get(expression)?.span();
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.non_transferable_delivery_code,
            "cross-thread delivery requires a Transferable value",
            primary,
        )?;
        let label = if let Some(lambda) = closure {
            self.captures_of(lambda)
                .find(|capture| {
                    capture.mode() == ClosureCaptureMode::Shared
                        || self.transferabilities.get(capture.ty().index()).copied()
                            != Some(Transferability::Transferable)
                })
                .map(|capture| capture.reference_span())
                .unwrap_or(self.parsed.ast().expressions().get(lambda)?.span())
        } else if let Some(symbol) = self.expression_root_symbol(expression)? {
            self.names.symbols()[symbol.index()].span()
        } else {
            primary
        };
        diagnostic.add_label(
            self.sources,
            label,
            "non-Transferable source originates here",
        )?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }
}
