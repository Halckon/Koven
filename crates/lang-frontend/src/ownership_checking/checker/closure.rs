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
        let mut captured_closures: Vec<ExpressionId> = Vec::new();
        for capture in &captures {
            if let ClosureCaptureSource::Symbol(symbol) = capture.source()
                && let Some(origins) = state.closures.get(&symbol)
            {
                captured_closures.extend(origins);
            }
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
                        // The owned environment now holds this closure's old value.
                        state.closures.remove(&symbol);
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

        captured_closures.sort_by_key(|origin| origin.index());
        captured_closures.dedup();
        state
            .closure_captures
            .insert(lambda.index(), captured_closures);

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
        let previous = self.current_receiver_mode;
        if previous.is_some() {
            // lambda 对 this 只能持有 shared capture，不能继承外层 Inout 能力。
            self.current_receiver_mode = Some(crate::type_checking::ParameterMode::Borrow);
        }
        let result = self.check_control_body(body, body_state, super::ExpressionUse::Consume, true);
        self.current_receiver_mode = previous;
        result?;
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

    pub(super) fn closure_origins(
        &self,
        expression: ExpressionId,
        state: &State,
    ) -> Result<Vec<ExpressionId>, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::Lambda { .. } => Ok(vec![expression]),
            Expression::Group { expression } => self.closure_origins(*expression, state),
            Expression::Name => Ok(self
                .reference_symbol(node.span())
                .and_then(|symbol| state.closures.get(&symbol).cloned())
                .unwrap_or_default()),
            _ => Ok(state
                .closure_results
                .get(&expression.index())
                .cloned()
                .unwrap_or_default()),
        }
    }

    pub(super) fn expression_root_symbol(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<SymbolId>, OwnershipCheckingError> {
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

    pub(super) fn release_closure(&self, symbol: SymbolId, state: &mut State) {
        // Last use within a replacement RHS does not finish the old environment's lifetime.
        if state.replacements.contains(&symbol) {
            return;
        }
        self.release_closure_except(symbol, state, &[]);
    }

    pub(super) fn release_closure_except(
        &self,
        symbol: SymbolId,
        state: &mut State,
        retained: &[ExpressionId],
    ) {
        let Some(origins) = state.closures.remove(&symbol) else {
            return;
        };
        for closure in origins {
            self.release_unheld_closure(closure, state, retained);
        }
    }

    fn release_unheld_closure(
        &self,
        closure: ExpressionId,
        state: &mut State,
        retained: &[ExpressionId],
    ) {
        let mut pending = vec![closure];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(closure) = pending.pop() {
            if !seen.insert(closure.index()) || self.closure_is_held(closure, state, retained) {
                continue;
            }
            state
                .loans
                .retain(|loan| loan.owner != ActiveLoanOwner::Closure(closure));
            if let Some(captured) = state.closure_captures.remove(&closure.index()) {
                pending.extend(captured);
            }
        }
    }

    fn closure_is_held(
        &self,
        closure: ExpressionId,
        state: &State,
        retained: &[ExpressionId],
    ) -> bool {
        let mut pending = retained.to_vec();
        pending.extend(state.closures.values().flatten().copied());
        pending.extend(state.pending_closures.values().flatten().copied());
        let mut seen = std::collections::BTreeSet::new();
        while let Some(current) = pending.pop() {
            if current == closure {
                return true;
            }
            if seen.insert(current.index())
                && let Some(captured) = state.closure_captures.get(&current.index())
            {
                pending.extend(captured);
            }
        }
        false
    }

    /// 已求值的 callee/实参仍由当前调用持有，不能按源码最后读取点提前结束 capture。
    pub(super) fn hold_call_closures(
        &self,
        call: ExpressionId,
        value: ExpressionId,
        flows: &mut Flows,
    ) -> Result<(), OwnershipCheckingError> {
        if let Some(state) = flows.next.as_mut() {
            let origins = self.closure_origins(value, state)?;
            if !origins.is_empty() {
                let pending = state.pending_closures.entry(call.index()).or_default();
                pending.extend(origins);
                pending.sort_by_key(|origin| origin.index());
                pending.dedup();
            }
        }
        Ok(())
    }

    pub(super) fn finish_call_closures(&self, call: ExpressionId, flows: &mut Flows) {
        for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
            .into_iter()
            .flatten()
        {
            if let Some(origins) = state.pending_closures.remove(&call.index()) {
                for closure in origins {
                    self.release_unheld_closure(closure, state, &[]);
                }
            }
        }
    }

    /// 合流后已死的 binding 释放 capture；结果值在建立接收 binding 前独立持有其 origins。
    pub(super) fn release_dead_control_closures(&self, control: ExpressionId, flows: &mut Flows) {
        let Some(state) = flows.next.as_mut() else {
            return;
        };
        let retained = state
            .closure_results
            .get(&control.index())
            .cloned()
            .unwrap_or_default();
        let dead = state
            .closures
            .keys()
            .copied()
            .filter(|symbol| {
                !self.expression_live_after[control.index()].contains(symbol)
                    && !state.replacements.contains(symbol)
            })
            .collect::<Vec<_>>();
        for symbol in dead {
            self.release_closure_except(symbol, state, &retained);
        }
    }

    /// Loop exits release dead local environments; an enclosing replacement still holds its root.
    pub(super) fn release_dead_loop_closures(
        &self,
        statement: crate::ast::StatementId,
        flows: &mut Flows,
    ) {
        let Some(state) = flows.next.as_mut() else {
            return;
        };
        loop {
            let dead = state
                .closures
                .keys()
                .copied()
                .filter(|symbol| {
                    !self.statement_live_after[statement.index()].contains(symbol)
                        && !state.replacements.contains(symbol)
                        && !state.loans.iter().any(|loan| {
                            matches!(&loan.target, ActiveLoanTarget::Place(place) if place.root() == *symbol)
                        })
                })
                .collect::<Vec<_>>();
            if dead.is_empty() {
                break;
            }
            // Releasing an environment can make its captured closure eligible next.
            for symbol in dead {
                self.release_closure(symbol, state);
            }
        }
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
        let mut pending = self.closure_origins(expression, state)?;
        let mut seen = std::collections::BTreeSet::new();
        let mut borrowed = None;
        while let Some(lambda) = pending.pop() {
            if !seen.insert(lambda.index()) {
                continue;
            }
            let captures = self.captures_of(lambda).collect::<Vec<_>>();
            if captures
                .iter()
                .any(|capture| capture.mode() == ClosureCaptureMode::Shared)
            {
                borrowed = Some(lambda);
                break;
            }
            if let Some(origins) = state.closure_captures.get(&lambda.index()) {
                pending.extend(origins);
            } else {
                // A directly delivered lambda is checked before its environment is formed.
                for capture in captures {
                    if let ClosureCaptureSource::Symbol(symbol) = capture.source()
                        && let Some(origins) = state.closures.get(&symbol)
                    {
                        pending.extend(origins);
                    }
                }
            }
        }
        let Some(lambda) = borrowed else {
            return Ok(());
        };
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
        let origins = self.closure_origins(expression, state)?;
        let closure = origins.iter().copied().find(|&lambda| {
            !self.closures.iter().any(|descriptor| {
                descriptor.expression() == lambda
                    && descriptor.transferability() == Transferability::Transferable
            })
        });
        let transferable = if origins.is_empty() {
            self.typed
                .expression_type(expression)
                .and_then(|ty| self.transferabilities.get(ty.index()).copied())
                == Some(Transferability::Transferable)
        } else {
            closure.is_none()
        };
        if transferable {
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
