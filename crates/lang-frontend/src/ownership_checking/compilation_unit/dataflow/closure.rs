//! Source-qualified closure formation、逃逸与跨线程交付。

use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Diagnostic, Severity},
    parser::{Expression, Statement},
    type_checking::UnitExpressionId,
};

use crate::ownership_checking::{
    ClosureCaptureEffect, ClosureCaptureMode, LoanKind, Transferability, UnitClosureCaptureSource,
    UnitOwnershipPlace,
};

use super::{
    AccessKind, ActiveLoan, ActiveLoanOwner, ActiveLoanTarget, Checker, Flows,
    OwnershipBindingKind, OwnershipCheckingError, State,
};

impl Checker<'_> {
    pub(super) fn check_lambda(
        &mut self,
        lambda: ExpressionId,
        body: crate::ast::StatementId,
        mut state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        self.visited_lambdas.insert(self.unit_expression(lambda));
        let diagnostic_count = self.diagnostics.len();
        let entry_state = state.clone();
        let lambda_id = self.unit_expression(lambda);
        let captures = self.captures_of(lambda).collect::<Vec<_>>();
        for capture in &captures {
            if let UnitClosureCaptureSource::Symbol(symbol) = capture.source()
                && state.borrow_bindings.contains_key(&symbol)
            {
                self.emit_borrow_binding_diagnostic(
                    crate::diagnostic::codes::UNSUPPORTED_BORROW_FLOW,
                    "borrow result capture continuation is not yet proven",
                    capture.reference_span(),
                )?;
            }
        }
        for capture in &captures {
            match (capture.mode(), capture.source(), capture.effect()) {
                (ClosureCaptureMode::Shared, UnitClosureCaptureSource::Symbol(symbol), _) => {
                    let place = UnitOwnershipPlace::new(symbol, Vec::new());
                    if self.access_place(
                        &place,
                        AccessKind::SharedLoan,
                        capture.reference_span(),
                        None,
                        &mut state,
                    )? {
                        state.loans.push(ActiveLoan {
                            owner: ActiveLoanOwner::Closure(lambda_id),
                            target: ActiveLoanTarget::Place(place),
                            kind: LoanKind::Shared,
                            reserved: false,
                            origin: capture.reference_span(),
                        });
                    }
                }
                (ClosureCaptureMode::Shared, UnitClosureCaptureSource::This, _) => {
                    if self.ensure_this_available_at(capture.reference_span(), None, &state)?
                        && self.access_this_at(
                            AccessKind::SharedLoan,
                            capture.reference_span(),
                            None,
                            &state,
                        )?
                    {
                        state.loans.push(ActiveLoan {
                            owner: ActiveLoanOwner::Closure(lambda_id),
                            target: ActiveLoanTarget::This,
                            kind: LoanKind::Shared,
                            reserved: false,
                            origin: capture.reference_span(),
                        });
                    }
                }
                (ClosureCaptureMode::Owned, UnitClosureCaptureSource::This, _) => {
                    self.emit_illegal_owned_capture(
                        capture.reference_span(),
                        capture.reference_span(),
                        "move closure cannot capture this or a field directly",
                    )?;
                }
                (
                    ClosureCaptureMode::Owned,
                    UnitClosureCaptureSource::Symbol(symbol),
                    ClosureCaptureEffect::Move,
                ) => {
                    let binding = self.symbol_span(symbol)?;
                    if self
                        .bindings
                        .get(&symbol)
                        .is_some_and(|binding| binding.kind() != OwnershipBindingKind::Owned)
                        || state.non_owning.contains_key(&symbol)
                    {
                        self.emit_illegal_owned_capture(
                            capture.reference_span(),
                            binding,
                            "move closure cannot own a non-Copyable borrowed capture",
                        )?;
                        continue;
                    }
                    let place = UnitOwnershipPlace::new(symbol, Vec::new());
                    if self.access_place(
                        &place,
                        AccessKind::Move,
                        capture.reference_span(),
                        None,
                        &mut state,
                    )? {
                        state.moved.insert(symbol, capture.reference_span());
                    }
                }
                (
                    ClosureCaptureMode::Owned,
                    UnitClosureCaptureSource::Symbol(symbol),
                    ClosureCaptureEffect::Copy | ClosureCaptureEffect::Unknown,
                ) => {
                    let place = UnitOwnershipPlace::new(symbol, Vec::new());
                    self.access_place(
                        &place,
                        AccessKind::Read,
                        capture.reference_span(),
                        None,
                        &mut state,
                    )?;
                }
                (
                    ClosureCaptureMode::Owned,
                    UnitClosureCaptureSource::Symbol(_),
                    ClosureCaptureEffect::Borrow,
                ) => {
                    return Err(OwnershipCheckingError::InvalidUnitClosureCapture {
                        source_unit: self.source_unit.index(),
                        expression: lambda.index(),
                    });
                }
            }
        }

        if self.diagnostics.len() != diagnostic_count {
            return Ok(Flows::next(entry_state));
        }

        let mut body_state = State::default();
        self.seed_lambda_parameters(lambda, &mut body_state)?;
        for capture in captures {
            match capture.source() {
                UnitClosureCaptureSource::This if capture.mode() == ClosureCaptureMode::Shared => {
                    body_state.loans.push(ActiveLoan {
                        owner: ActiveLoanOwner::Closure(lambda_id),
                        target: ActiveLoanTarget::This,
                        kind: LoanKind::Shared,
                        reserved: false,
                        origin: capture.reference_span(),
                    });
                }
                UnitClosureCaptureSource::This => {}
                UnitClosureCaptureSource::Symbol(symbol) => {
                    body_state
                        .immutable_captures
                        .insert(symbol, capture.reference_span());
                    if capture.mode() == ClosureCaptureMode::Shared {
                        body_state
                            .non_owning
                            .insert(symbol, capture.reference_span());
                    }
                }
            }
        }
        self.check_statement(body, body_state)?;
        if self.diagnostics.len() != diagnostic_count {
            return Ok(Flows::next(entry_state));
        }
        Ok(Flows::next(state))
    }

    fn emit_illegal_owned_capture(
        &mut self,
        primary: crate::source::Span,
        binding: crate::source::Span,
        message: &'static str,
    ) -> Result<(), OwnershipCheckingError> {
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.codes.illegal_owned_capture,
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
    ) -> Result<Option<UnitExpressionId>, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::Lambda { .. } => Ok(Some(self.unit_expression(expression))),
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
    ) -> Result<Option<crate::name_resolution::UnitSymbolId>, OwnershipCheckingError> {
        if self.is_constant_use(expression) {
            return Ok(None);
        }
        let node = self.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::Name => Ok(self.reference_symbol(node.span())),
            Expression::Group { expression } => self.expression_root_symbol(*expression),
            _ => Ok(None),
        }
    }

    pub(super) fn release_closure(
        &self,
        symbol: crate::name_resolution::UnitSymbolId,
        state: &mut State,
    ) {
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
        if let Some(symbol) = self.expression_root_symbol(expression)? {
            if self.expression_live_after[expression.index()].contains(&symbol) {
                return Ok(());
            }
            for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
                .into_iter()
                .flatten()
            {
                self.release_closure(symbol, state);
            }
        } else {
            let mut temporaries = Vec::new();
            self.direct_closure_origins(expression, &mut temporaries)?;
            for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
                .into_iter()
                .flatten()
            {
                for &closure in &temporaries {
                    if !state.closures.values().any(|&other| other == closure) {
                        state
                            .loans
                            .retain(|loan| loan.owner != ActiveLoanOwner::Closure(closure));
                    }
                }
            }
        }
        Ok(())
    }

    /// Only anonymous result temporaries end with this expression's completed use.
    /// A named branch result keeps its existing owner/capture lifetime.
    fn direct_closure_origins(
        &self,
        expression: ExpressionId,
        origins: &mut Vec<UnitExpressionId>,
    ) -> Result<(), OwnershipCheckingError> {
        match self.parsed.ast().expressions().get(expression)?.payload() {
            Expression::Lambda { .. } => origins.push(self.unit_expression(expression)),
            Expression::Group { expression } => {
                self.direct_closure_origins(*expression, origins)?
            }
            Expression::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.direct_closure_body_origins(*then_branch, origins)?;
                if let Some(branch) = else_branch {
                    self.direct_closure_body_origins(*branch, origins)?;
                }
            }
            Expression::When { entries, .. } => {
                for entry in entries {
                    self.direct_closure_body_origins(entry.body, origins)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn direct_closure_body_origins(
        &self,
        body: StatementId,
        origins: &mut Vec<UnitExpressionId>,
    ) -> Result<(), OwnershipCheckingError> {
        match self.parsed.ast().statements().get(body)?.payload() {
            Statement::Expression { expression } => {
                self.direct_closure_origins(*expression, origins)?
            }
            Statement::ControlBody { elements } => {
                if let Some(&tail) = elements.last() {
                    self.direct_closure_body_origins(tail, origins)?;
                }
            }
            _ => {}
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
        if !self.captures.iter().any(|capture| {
            capture.lambda() == lambda && capture.mode() == ClosureCaptureMode::Shared
        }) {
            return Ok(());
        }
        let primary = self.parsed.ast().expressions().get(expression)?.span();
        let lambda_span = self
            .parsed
            .ast()
            .expressions()
            .get(lambda.expression())?
            .span();
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.codes.borrowed_closure_escape,
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
                    .expression_type(self.unit_expression(expression))
                    .and_then(|ty| self.transferabilities.get(ty.index()).copied())
            });
        if transferability == Some(Transferability::Transferable) {
            return Ok(());
        }
        let primary = self.parsed.ast().expressions().get(expression)?.span();
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.codes.non_transferable_delivery,
            "cross-thread delivery requires a Transferable value",
            primary,
        )?;
        let label = if let Some(lambda) = closure {
            self.captures
                .iter()
                .filter(|capture| capture.lambda() == lambda)
                .find(|capture| {
                    capture.mode() == ClosureCaptureMode::Shared
                        || self.transferabilities.get(capture.ty().index()).copied()
                            != Some(Transferability::Transferable)
                })
                .map(|capture| capture.reference_span())
                .unwrap_or(
                    self.parsed
                        .ast()
                        .expressions()
                        .get(lambda.expression())?
                        .span(),
                )
        } else if let Some(symbol) = self.expression_root_symbol(expression)? {
            self.symbol_span(symbol)?
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

    pub(super) fn captures_of(
        &self,
        lambda: ExpressionId,
    ) -> impl Iterator<Item = crate::ownership_checking::UnitClosureCaptureDescriptor> + '_ {
        let lambda = self.unit_expression(lambda);
        self.captures
            .iter()
            .copied()
            .filter(move |capture| capture.lambda() == lambda)
    }
}
