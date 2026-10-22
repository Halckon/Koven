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
        let statement_id = UnitStatementId::new(self.source_unit, statement);
        if self
            .typed
            .diagnostic_iteration_header_rejected(statement_id)
        {
            let errors = self.diagnostics.len();
            let mut prefix = self.check_expression(source, state, ExpressionUse::Read)?;
            let Some(base) = prefix.next.as_mut() else {
                return Ok(prefix);
            };
            base.origins.attach(&self.callable_sources.arena);
            let headers = base.origins.begin_loop();
            return self.check_maybe_loop(prefix, body, errors, &headers);
        }
        if !crate::type_checking::map_iteration_validation::validate_unit_map_iteration_facts(
            self.parsed,
            self.names,
            self.typed,
            statement_id,
            self.typed.diagnostic_map_iteration(statement_id),
            self.typed.diagnostic_sequential_iteration(statement_id),
        ) {
            return Err(OwnershipCheckingError::InvalidMapIteration {
                source_unit: Some(self.source_unit.index()),
                statement: statement.index(),
            });
        }
        let map_descriptor = self.typed.diagnostic_map_iteration(statement_id).copied();
        let numeric = self
            .typed
            .diagnostic_sequential_iteration(UnitStatementId::new(self.source_unit, statement))
            .is_some_and(|plan| plan.delivery() == crate::type_checking::ParameterMode::Value);
        let errors = self.diagnostics.len();
        let previous = self.allowed_borrow_call;
        if self.range_expression_is_proven(source) {
            self.allowed_borrow_call = Some(source);
        }
        let checked = if map_descriptor.is_some() {
            self.check_map_iteration_source(source, state)
        } else {
            self.check_expression(
                source,
                state,
                if numeric {
                    ExpressionUse::Read
                } else {
                    ExpressionUse::Place {
                        parameter_span: None,
                    }
                },
            )
        };
        self.allowed_borrow_call = previous;
        let mut prefix = checked?;
        if self.diagnostics.len() == errors && self.range_expression_is_proven(source) {
            self.record_range_use(source, crate::ownership_checking::RangeUseSite::Iteration);
            self.continue_range_source(
                source,
                ActiveLoanOwner::IterationSource(UnitStatementId::new(self.source_unit, statement)),
                &mut prefix,
            );
        }
        let Some(base) = prefix.next.as_mut() else {
            return Ok(prefix);
        };
        if map_descriptor.is_some() && self.diagnostics.len() != errors {
            base.origins.attach(&self.callable_sources.arena);
            let headers = base.origins.begin_loop();
            return self.check_maybe_loop(prefix, body, errors, &headers);
        }
        let statement = UnitStatementId::new(self.source_unit, statement);
        let descriptor = self
            .typed
            .diagnostic_sequential_iteration(statement)
            .cloned();
        if descriptor.is_none() && map_descriptor.is_none() {
            return Err(OwnershipCheckingError::InvalidUnitArgumentType {
                source_unit: self.source_unit.index(),
                expression: source.index(),
            });
        }
        let span = self.parsed.ast().expressions().get(source)?.span();
        let map_source_access = if map_descriptor.is_some() {
            self.place(source)?
                .filter(|place| base.non_owning.contains_key(&place.root()))
                .map(|_| UnitIterationSourceAccess::Shared)
        } else {
            None
        };
        let target = if numeric {
            None
        } else {
            Some(if let Some(fact) = self.range_use(source) {
                fact.origin().clone()
            } else if let Some(place) = self.place(source)? {
                let place = if map_descriptor.is_some() {
                    self.canonical_borrow_place(&place, base)
                } else {
                    place
                };
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
                let owner = if map_descriptor.is_some() {
                    self.map_iteration_temporary_owner(source)?
                } else {
                    self.temporary_projection_owner(source)?
                        .or(self.temporary_expression_origin(source)?)
                }
                .ok_or(OwnershipCheckingError::InvalidUnitArgumentPlace {
                    source_unit: self.source_unit.index(),
                    expression: source.index(),
                })?;
                UnitLoanTarget::Temporary(owner)
            })
        };
        let source_access = map_source_access.unwrap_or(match target.as_ref() {
            None => UnitIterationSourceAccess::Owned,
            Some(UnitLoanTarget::Place(place)) if base.non_owning.contains_key(&place.root()) => {
                UnitIterationSourceAccess::Shared
            }
            Some(UnitLoanTarget::Place(place))
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
            Some(UnitLoanTarget::Place(place)) => self.bindings.get(&place.root()).map_or(
                UnitIterationSourceAccess::Owned,
                |binding| match binding.kind() {
                    OwnershipBindingKind::Owned => UnitIterationSourceAccess::Owned,
                    OwnershipBindingKind::Shared => UnitIterationSourceAccess::Shared,
                    OwnershipBindingKind::Exclusive => UnitIterationSourceAccess::Exclusive,
                },
            ),
            Some(UnitLoanTarget::This(_)) => UnitIterationSourceAccess::Shared,
            Some(UnitLoanTarget::Temporary(_)) => UnitIterationSourceAccess::Temporary,
        });
        let symbols = if let Some(plan) = map_descriptor {
            plan.bindings().to_vec()
        } else {
            descriptor
                .as_ref()
                .map(|plan| plan.binding().symbols().collect())
                .unwrap_or_default()
        };
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
        for (index, binding) in bindings.iter().enumerate() {
            let symbol = binding.symbol();
            base.moved.remove(&symbol);
            if !numeric {
                base.non_owning.insert(symbol, binding.declaration_span());
                base.loans.push(ActiveLoan {
                    owner: if map_descriptor.is_some() {
                        ActiveLoanOwner::MapIterationBinding {
                            statement,
                            source: self.unit_expression(source),
                            component: if index == 0 {
                                crate::ownership_checking::MapIterationComponent::Key
                            } else {
                                crate::ownership_checking::MapIterationComponent::Value
                            },
                        }
                    } else {
                        ActiveLoanOwner::IterationElement(statement)
                    },
                    target: ActiveLoanTarget::Place(UnitOwnershipPlace::new(symbol, Vec::new())),
                    kind: LoanKind::Shared,
                    reserved: false,
                    origin: binding.declaration_span(),
                });
            }
        }
        if let Some(descriptor) = map_descriptor {
            let origin = target.ok_or(OwnershipCheckingError::InvalidMapIteration {
                source_unit: Some(self.source_unit.index()),
                statement: statement.statement().index(),
            })?;
            let bindings: [crate::ownership_checking::UnitOwnershipBindingDescriptor; 2] = bindings
                .try_into()
                .map_err(|_| OwnershipCheckingError::InvalidMapIteration {
                    source_unit: Some(self.source_unit.index()),
                    statement: statement.statement().index(),
                })?;
            let loans = [
                crate::ownership_checking::MapIterationComponent::Key,
                crate::ownership_checking::MapIterationComponent::Value,
            ]
            .map(|component| crate::ownership_checking::MapIterationLoan {
                statement,
                source: descriptor.source(),
                component,
                binding: match component {
                    crate::ownership_checking::MapIterationComponent::Key => {
                        descriptor.key_binding()
                    }
                    crate::ownership_checking::MapIterationComponent::Value => {
                        descriptor.value_binding()
                    }
                },
                origin: origin.clone(),
            });
            if self.diagnostics.len() == errors {
                self.map_iterations.insert(
                    statement,
                    crate::ownership_checking::UnitMapIterationOwnershipPlan {
                        descriptor,
                        source: crate::ownership_checking::IterationSource::BorrowedSource(origin),
                        source_access,
                        bindings,
                        loans,
                        exits: Vec::new(),
                    },
                );
            }
        } else if let Some(descriptor) = descriptor {
            self.iterations.insert(
                statement,
                UnitIterationOwnershipPlan {
                    descriptor,
                    source: target.map_or_else(
                        || {
                            crate::ownership_checking::IterationSource::OwnedSnapshot(
                                self.unit_expression(source),
                            )
                        },
                        crate::ownership_checking::IterationSource::BorrowedSource,
                    ),
                    source_access,
                    bindings,
                    exits: Vec::new(),
                },
            );
        }
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
                && !matches!(loan.owner, ActiveLoanOwner::MapIterationBinding { statement: owner, .. } if owner == statement)
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
        let closures = state.closure_tree(
            live.iter()
                .filter_map(|symbol| state.closures.get(symbol))
                .flatten()
                .copied(),
        );
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
