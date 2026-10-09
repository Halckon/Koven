//! for 借用能力独立于普通 call；同一 provider 覆盖 body 与 backedge。
use super::loan::{ActiveLoan, ActiveLoanOwner, ActiveLoanTarget};
use super::{
    AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State, merge_state,
};
use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Diagnostic, Severity},
    name_resolution::SymbolId,
    ownership_checking::{LoanKind, OwnershipPlace},
    source::Span,
    type_checking::SequentialIterationBinding,
};
use std::collections::BTreeSet;

impl Checker<'_> {
    pub(super) fn check_iteration(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        body: StatementId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let errors = self.diagnostics.len();
        let previous = self.allowed_borrow_call;
        if self.range_expression_is_proven(source) {
            self.allowed_borrow_call = Some(source);
        }
        let checked = self.check_expression(source, state, ExpressionUse::Place);
        self.allowed_borrow_call = previous;
        let mut prefix = checked?;
        if self.diagnostics.len() == errors && self.range_expression_is_proven(source) {
            self.record_range_use(source, crate::ownership_checking::RangeUseSite::Iteration);
            self.continue_range_source(
                source,
                ActiveLoanOwner::IterationSource(statement),
                &mut prefix,
            );
        }
        let Some(state) = prefix.next.as_mut() else {
            return Ok(prefix);
        };
        let Some(plan) = self.typed.sequential_iteration(statement).cloned() else {
            // 上游类型错误或 Deferred source 尚无可消费的 provider。
            return self.check_maybe_loop(prefix, body, errors);
        };
        let source_span = self.parsed.ast().expressions().get(source)?.span();
        if self.diagnostics.len() == errors
            && let Some(place) = self.place(source)?
            && self.access_place(&place, AccessKind::SharedLoan, false, source_span, state)?
        {
            state.loans.push(ActiveLoan {
                owner: ActiveLoanOwner::IterationSource(statement),
                target: ActiveLoanTarget::Place(place),
                kind: LoanKind::Shared,
                origin: source_span,
            });
        }
        let bindings: Vec<SymbolId> = match plan.binding() {
            SequentialIterationBinding::Discard => Vec::new(),
            SequentialIterationBinding::Name(symbol) => vec![*symbol],
            SequentialIterationBinding::Destructure(components) => components
                .iter()
                .filter_map(|component| component.symbol())
                .collect(),
        };
        for &symbol in &bindings {
            let origin = self.names.symbols()[symbol.index()].span();
            state.moved.remove(&symbol);
            state.non_owning.insert(symbol, origin);
            // binding 有独立借用身份；字段 mutation 不能绕过 Shared element access。
            state.loans.push(ActiveLoan {
                owner: ActiveLoanOwner::IterationElement(statement),
                target: ActiveLoanTarget::Place(OwnershipPlace::new(symbol, Vec::new())),
                kind: LoanKind::Shared,
                origin,
            });
        }
        self.iterations.insert(
            statement.index(),
            crate::ownership_checking::IterationOwnershipPlan {
                descriptor: plan,
                source: self
                    .range_use(source)
                    .map(|fact| fact.origin().clone())
                    .or(self
                        .place(source)?
                        .map(crate::ownership_checking::LoanTarget::Place))
                    .unwrap_or(crate::ownership_checking::LoanTarget::Temporary(
                        self.temporary_element_owner(source)?.unwrap_or(source),
                    )),
                bindings: bindings
                    .iter()
                    .map(|&symbol| {
                        crate::ownership_checking::OwnershipBindingDescriptor::new(
                            symbol,
                            crate::ownership_checking::OwnershipBindingKind::Shared,
                        )
                    })
                    .collect(),
                exits: Vec::new(),
                closure_flow: Default::default(),
                capture_graph: Default::default(),
                closure_phis: Vec::new(),
                closure_phi_incomings: Vec::new(),
            },
        );
        let Some(mut base) = prefix.next.take() else {
            return Ok(prefix);
        };
        base.origins.attach(&self.callable_sources.arena);
        let headers = base.origins.begin_loop();
        let body_flows = self.check_statement(body, base.clone())?;
        if self.diagnostics.len() == errors {
            self.check_loop_backedge(body, &body_flows)?;
            let backedge_live = self.statement_live_after[body.index()].clone();
            for state in [&body_flows.next, &body_flows.continues]
                .into_iter()
                .flatten()
            {
                self.reject_surviving_element_closure(
                    state,
                    &backedge_live,
                    &bindings,
                    source_span,
                )?;
            }
            if let Some(state) = &body_flows.breaks {
                let exit_live = self.statement_live_after[statement.index()].clone();
                self.reject_surviving_element_closure(state, &exit_live, &bindings, source_span)?;
            }
        }
        let origins = base.origins.loop_exit(
            &headers,
            body_flows.next.as_ref().map(|state| &state.origins),
            body_flows.continues.as_ref().map(|state| &state.origins),
            body_flows.breaks.as_ref().map(|state| &state.origins),
        );
        let mut next = base;
        for state in [body_flows.next, body_flows.breaks, body_flows.continues]
            .into_iter()
            .flatten()
        {
            merge_state(&mut next, state);
        }
        next.origins = origins;
        prefix.next = Some(next);
        let mut flows = prefix;
        for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
            .into_iter()
            .flatten()
        {
            state.loans.retain(|loan| {
                loan.owner != ActiveLoanOwner::IterationSource(statement)
                    && loan.owner != ActiveLoanOwner::IterationElement(statement)
            });
            for symbol in &bindings {
                state.non_owning.remove(symbol);
                state.moved.remove(symbol);
            }
        }
        Ok(flows)
    }

    fn reject_surviving_element_closure(
        &mut self,
        state: &State,
        live: &BTreeSet<SymbolId>,
        bindings: &[SymbolId],
        boundary: Span,
    ) -> Result<(), OwnershipCheckingError> {
        let mut pending = live
            .iter()
            .filter_map(|symbol| state.closures.get(symbol))
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        while let Some(lambda) = pending.pop() {
            if !seen.insert(lambda.index()) {
                continue;
            }
            if let Some(loan) = state.loans.iter().find(|loan| {
                loan.owner == ActiveLoanOwner::Closure(lambda)
                    && matches!(&loan.target, ActiveLoanTarget::Place(place) if bindings.contains(&place.root()))
            }) {
                let mut diagnostic = Diagnostic::new(
                    self.sources,
                    Severity::Error,
                    self.borrowed_closure_escape_code,
                    "borrowed closure cannot outlive its iteration element",
                    boundary,
                )?;
                diagnostic.add_label(self.sources, loan.origin, "iteration element captured here")?;
                self.diagnostics.push(diagnostic);
                return Ok(());
            }
            if let Some(captured) = state.closure_captures.get(&lambda.index()) {
                pending.extend(captured);
            }
        }
        Ok(())
    }
}
