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

use std::collections::BTreeSet;

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
        let captured_closures = captures
            .iter()
            .filter_map(|capture| {
                if let UnitClosureCaptureSource::Symbol(symbol) = capture.source() {
                    state.closures.get(&symbol)
                } else {
                    None
                }
            })
            .flatten()
            .copied()
            .collect();
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
                        // The nested environment now belongs to the formed outer value.
                        state.closures.remove(&symbol);
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
        state.closure_captures.insert(lambda_id, captured_closures);
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

    pub(super) fn closure_origins(
        &self,
        expression: ExpressionId,
        state: &State,
    ) -> Result<BTreeSet<UnitExpressionId>, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        Ok(match node.payload() {
            Expression::Lambda { .. } => BTreeSet::from([self.unit_expression(expression)]),
            Expression::Group { expression } => self.closure_origins(*expression, state)?,
            Expression::Name => self
                .reference_symbol(node.span())
                .and_then(|s| state.closures.get(&s).cloned())
                .unwrap_or_default(),
            _ => state
                .closure_results
                .get(&self.unit_expression(expression))
                .cloned()
                .unwrap_or_default(),
        })
    }

    pub(super) fn closure_result_origins(
        &self,
        expression: ExpressionId,
        state: &State,
    ) -> Result<BTreeSet<UnitExpressionId>, OwnershipCheckingError> {
        match self.parsed.ast().expressions().get(expression)?.payload() {
            Expression::If {
                then_branch,
                else_branch,
                ..
            } => {
                let mut result = self.closure_body_origins(*then_branch, state)?;
                if let Some(body) = else_branch {
                    result.extend(self.closure_body_origins(*body, state)?);
                }
                Ok(result)
            }
            Expression::When { entries, .. } => {
                let mut result = BTreeSet::new();
                for entry in entries {
                    result.extend(self.closure_body_origins(entry.body, state)?);
                }
                Ok(result)
            }
            _ => self.closure_origins(expression, state),
        }
    }

    fn closure_body_origins(
        &self,
        body: StatementId,
        state: &State,
    ) -> Result<BTreeSet<UnitExpressionId>, OwnershipCheckingError> {
        match self.parsed.ast().statements().get(body)?.payload() {
            Statement::Expression { expression } => Ok(state
                .closure_results
                .get(&self.unit_expression(*expression))
                .cloned()
                .unwrap_or_default()),
            Statement::ControlBody { elements } => {
                if let Some(tail) = elements.last() {
                    self.closure_body_origins(*tail, state)
                } else {
                    Ok(BTreeSet::new())
                }
            }
            _ => Ok(BTreeSet::new()),
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
        if !state.replacements.contains(&symbol) {
            self.release_closure_except(symbol, &BTreeSet::new(), state);
        }
    }

    pub(super) fn release_closure_except(
        &self,
        symbol: crate::name_resolution::UnitSymbolId,
        retained: &BTreeSet<UnitExpressionId>,
        state: &mut State,
    ) {
        let Some(closure) = state.closures.remove(&symbol) else {
            return;
        };
        for origin in closure {
            self.release_unheld_closure(origin, retained, state);
        }
    }

    fn release_unheld_closure(
        &self,
        closure: UnitExpressionId,
        retained: &BTreeSet<UnitExpressionId>,
        state: &mut State,
    ) {
        let mut pending = vec![closure];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(current) = pending.pop() {
            if !seen.insert(current)
                || state
                    .closure_tree(
                        state
                            .closures
                            .values()
                            .flatten()
                            .copied()
                            .chain(retained.iter().copied()),
                    )
                    .contains(&current)
            {
                continue;
            }
            state
                .loans
                .retain(|loan| loan.owner != ActiveLoanOwner::Closure(current));
            if let Some(captured) = state.closure_captures.remove(&current) {
                pending.extend(captured);
            }
        }
    }

    pub(super) fn release_dead_control_closures(
        &self,
        control: ExpressionId,
        retained: &BTreeSet<UnitExpressionId>,
        state: &mut State,
    ) {
        loop {
            let dead=state.closures.keys().copied().filter(|symbol| {
                state.moved.contains_key(symbol) && !self.expression_live_after[control.index()].contains(symbol) && !state.replacements.contains(symbol)
                    && !state.loans.iter().any(|loan|matches!(&loan.target,ActiveLoanTarget::Place(place) if place.root()==*symbol))
            }).collect::<Vec<_>>();
            if dead.is_empty() {
                break;
            }
            for symbol in dead {
                self.release_closure_except(symbol, retained, state);
            }
        }
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
            for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
                .into_iter()
                .flatten()
            {
                // The completed argument owns its actual result, including a consumed branch-local tail.
                let temporaries = self.closure_origins(expression, state)?;
                for closure in temporaries {
                    self.release_unheld_closure(closure, &BTreeSet::new(), state);
                }
            }
        }
        Ok(())
    }

    pub(super) fn reject_borrowed_closure_escape(
        &mut self,
        expression: ExpressionId,
        state: &State,
    ) -> Result<(), OwnershipCheckingError> {
        let mut pending = self
            .closure_origins(expression, state)?
            .into_iter()
            .collect::<Vec<_>>();
        let mut seen = std::collections::BTreeSet::new();
        let mut borrowed = None;
        while let Some(lambda) = pending.pop() {
            if !seen.insert(lambda) {
                continue;
            }
            let captures = self
                .captures
                .iter()
                .filter(|c| c.lambda() == lambda)
                .collect::<Vec<_>>();
            if captures
                .iter()
                .any(|c| c.mode() == ClosureCaptureMode::Shared)
            {
                borrowed = Some(lambda);
                break;
            }
            if let Some(captured) = state.closure_captures.get(&lambda) {
                pending.extend(captured);
            } else {
                // Directly delivered lambdas are checked before their environment is formed.
                pending.extend(
                    captures
                        .iter()
                        .filter_map(|c| {
                            if let UnitClosureCaptureSource::Symbol(s) = c.source() {
                                state.closures.get(&s)
                            } else {
                                None
                            }
                        })
                        .flatten()
                        .copied(),
                );
            }
        }
        let Some(lambda) = borrowed else {
            return Ok(());
        };
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
        let origins = self.closure_origins(expression, state)?;
        let closure = (origins.len() == 1).then(|| *origins.first().unwrap());
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
