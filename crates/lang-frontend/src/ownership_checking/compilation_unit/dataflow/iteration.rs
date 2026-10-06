//! Source与element的独立Shared loan覆盖完整body/backedge，绝不消费element。
use super::{
    AccessKind, ActiveLoan, ActiveLoanOwner, ActiveLoanTarget, Checker, ExpressionUse, Flows,
    LoanKind, OwnershipCheckingError, State, merge_state,
};
use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Diagnostic, Severity},
    name_resolution::UnitSymbolId,
    ownership_checking::{
        OwnershipBindingKind, UnitIterationOwnershipPlan, UnitIterationSourceAccess,
        UnitLoanTarget, UnitOwnershipPlace,
    },
    source::Span,
    type_checking::UnitStatementId,
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
        let mut prefix = self.check_expression(
            source,
            state,
            ExpressionUse::Place {
                parameter_span: None,
            },
        )?;
        let Some(base) = prefix.next.as_mut() else {
            return Ok(prefix);
        };
        let statement = UnitStatementId::new(self.source_unit, statement);
        let descriptor = self.typed.sequential_iteration(statement).cloned().ok_or(
            OwnershipCheckingError::InvalidUnitArgumentType {
                source_unit: self.source_unit.index(),
                expression: source.index(),
            },
        )?;
        let span = self.parsed.ast().expressions().get(source)?.span();
        let target = if let Some(place) = self.place(source)? {
            if self.access_place(&place, AccessKind::SharedLoan, span, None, base)? {
                base.loans.push(ActiveLoan {
                    owner: ActiveLoanOwner::IterationSource(statement),
                    target: ActiveLoanTarget::Place(place.clone()),
                    kind: LoanKind::Shared,
                    reserved: false,
                    origin: span,
                });
            }
            UnitLoanTarget::Place(place)
        } else {
            let owner = self
                .temporary_projection_owner(source)?
                .or(self.temporary_expression_origin(source)?)
                .ok_or(OwnershipCheckingError::InvalidUnitArgumentPlace {
                    source_unit: self.source_unit.index(),
                    expression: source.index(),
                })?;
            UnitLoanTarget::Temporary(owner)
        };
        let source_access = match &target {
            UnitLoanTarget::Place(place) if base.non_owning.contains_key(&place.root()) => {
                UnitIterationSourceAccess::Shared
            }
            UnitLoanTarget::Place(place)
                if self.symbol_kind(place.root())
                    == Some(crate::name_resolution::SymbolKind::Field) =>
            {
                match self.current_receiver.map(|receiver| receiver.mode) {
                    Some(crate::type_checking::ParameterMode::Inout) => {
                        UnitIterationSourceAccess::Exclusive
                    }
                    Some(crate::type_checking::ParameterMode::Value) => {
                        UnitIterationSourceAccess::Owned
                    }
                    _ => UnitIterationSourceAccess::Shared,
                }
            }
            UnitLoanTarget::Place(place) => self.bindings.get(&place.root()).map_or(
                UnitIterationSourceAccess::Owned,
                |binding| match binding.kind() {
                    OwnershipBindingKind::Owned => UnitIterationSourceAccess::Owned,
                    OwnershipBindingKind::Shared => UnitIterationSourceAccess::Shared,
                    OwnershipBindingKind::Exclusive => UnitIterationSourceAccess::Exclusive,
                },
            ),
            UnitLoanTarget::This(_) => UnitIterationSourceAccess::Shared,
            UnitLoanTarget::Temporary(_) => UnitIterationSourceAccess::Temporary,
        };
        let symbols = descriptor.binding().symbols().collect::<Vec<_>>();
        let bindings = symbols
            .iter()
            .map(|symbol| {
                self.bindings.get(symbol).copied().ok_or(
                    OwnershipCheckingError::InvalidUnitSymbol {
                        source_unit: symbol.source_unit().index(),
                        symbol: symbol.symbol().index(),
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        for binding in &bindings {
            let symbol = binding.symbol();
            base.moved.remove(&symbol);
            base.non_owning.insert(symbol, binding.declaration_span());
            base.loans.push(ActiveLoan {
                owner: ActiveLoanOwner::IterationElement(statement),
                target: ActiveLoanTarget::Place(UnitOwnershipPlace::new(symbol, Vec::new())),
                kind: LoanKind::Shared,
                reserved: false,
                origin: binding.declaration_span(),
            });
        }
        self.iterations.insert(
            statement,
            UnitIterationOwnershipPlan {
                descriptor,
                source: target,
                source_access,
                bindings,
                exits: Vec::new(),
            },
        );
        let Some(mut next) = prefix.next.take() else {
            return Ok(prefix);
        };
        next.origins.attach(&self.callable_sources.arena);
        let headers = next.origins.begin_loop();
        let body_flows = self.check_statement(body, next.clone())?;
        if self.diagnostics.len() == errors {
            self.check_loop_backedge(body, &body_flows)?;
            for state in [&body_flows.next, &body_flows.continues]
                .into_iter()
                .flatten()
            {
                self.reject_surviving_iteration_capture(
                    state,
                    &self.statement_live_after[body.index()].clone(),
                    &symbols,
                    span,
                )?;
            }
            if let Some(state) = &body_flows.breaks {
                self.reject_surviving_iteration_capture(
                    state,
                    &self.statement_live_after[statement.statement().index()].clone(),
                    &symbols,
                    span,
                )?;
            }
        }
        let origins = next.origins.loop_exit(
            &headers,
            body_flows.next.as_ref().map(|state| &state.origins),
            body_flows.continues.as_ref().map(|state| &state.origins),
            body_flows.breaks.as_ref().map(|state| &state.origins),
        );
        for state in [body_flows.next, body_flows.breaks, body_flows.continues]
            .into_iter()
            .flatten()
        {
            merge_state(&mut next, state);
        }
        next.origins = origins;
        next.loans.retain(|loan| {
            loan.owner != ActiveLoanOwner::IterationSource(statement)
                && loan.owner != ActiveLoanOwner::IterationElement(statement)
        });
        for symbol in symbols {
            next.non_owning.remove(&symbol);
            next.moved.remove(&symbol);
        }
        prefix.next = Some(next);
        Ok(prefix)
    }

    fn reject_surviving_iteration_capture(
        &mut self,
        state: &State,
        live: &BTreeSet<UnitSymbolId>,
        bindings: &[UnitSymbolId],
        boundary: Span,
    ) -> Result<(), OwnershipCheckingError> {
        let closures = live
            .iter()
            .filter_map(|symbol| state.closures.get(symbol))
            .collect::<BTreeSet<_>>();
        if let Some(loan) = state.loans.iter().find(|loan| {
            matches!(&loan.owner, ActiveLoanOwner::Closure(closure) if closures.contains(closure))
                && matches!(&loan.target, ActiveLoanTarget::Place(place) if bindings.contains(&place.root()))
        }) {
            let mut diagnostic = Diagnostic::new(self.sources, Severity::Error,
                self.codes.borrowed_closure_escape, "borrowed closure cannot outlive its iteration element", boundary)?;
            diagnostic.add_label(self.sources, loan.origin, "iteration element captured here")?;
            self.diagnostics.push(diagnostic);
        }
        Ok(())
    }
}
