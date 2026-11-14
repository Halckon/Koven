//! Original declaration/body planning, with opt-in successful append observations.
use super::*;
impl DropPlanner<'_, '_> {
    pub(super) fn item(&mut self, id: ItemId) -> Result<(), OwnershipCheckingError> {
        match self.checker.parsed.ast().items().get(id)?.payload().clone() {
            Item::Modified { declaration, .. } => self.item(declaration)?,
            Item::Function {
                name,
                parameters,
                form,
                ..
            } => {
                if self.liveness.skipped_functions.contains(&id.index()) {
                    return Ok(());
                }
                #[cfg(feature = "d10-catalog-experiment")]
                let drop_start = (self.facts.len(), self.conditional_receiver_facts.len());
                self.begin_owner_function(id);
                let mut state = ValueState::default();
                if self.checker.receiver_context(name).is_some() {
                    self.reject_owner_callable(id);
                }
                if let Some(receiver) = self.checker.receiver_context(name)
                    && receiver.mode == ParameterMode::Value
                {
                    let conditional_type = match self.checker.typed.types().get(receiver.ty) {
                        Some(UnitTypeKind::StaticSelf(_)) => Some(receiver.ty),
                        _ => None,
                    };
                    if conditional_type.is_some()
                        || self.checker.typed.copyability(receiver.ty) == Copyability::MoveOnly
                    {
                        state.this = Some(OwnedThis {
                            owner: receiver.owner,
                            origin: receiver.declaration_span,
                            conditional_type,
                        });
                    }
                }
                for parameter in parameters {
                    if let Some(symbol) = self.checker.marker_symbol(parameter.name).copied()
                        && self.checker.is_move_only_variable(symbol)
                    {
                        self.binding_depths.insert(symbol, 0);
                        state.insert(OwnedValue {
                            owner: self.parameter_owner(id, symbol),
                            symbol,
                            origin: marker_span(parameter.name),
                            declaration: marker_span(parameter.name),
                            scope_depth: 0,
                        });
                    }
                }
                let live_in = self
                    .liveness
                    .function_live_in
                    .get(&id.index())
                    .cloned()
                    .unwrap_or_default();
                let has_body = !matches!(
                    form,
                    FunctionForm::ImplicitUnitAbsent
                        | FunctionForm::Explicit {
                            body: FunctionBody::Absent,
                            ..
                        }
                );
                if !has_body {
                    self.reject_owner_callable(id);
                }
                if has_body {
                    let unused_parameters = state
                        .values
                        .iter()
                        .filter(|value| !live_in.contains(&value.symbol))
                        .map(|value| value.symbol)
                        .collect::<Vec<_>>();
                    for symbol in unused_parameters.into_iter().rev() {
                        self.drop_named_asap(
                            PlannerDropPoint::FunctionEntry(id),
                            symbol,
                            &mut state,
                        );
                    }
                }
                match form {
                    FunctionForm::ImplicitUnitAbsent => {}
                    FunctionForm::ImplicitUnitBlock(body) => {
                        if self.statement(body, &mut state)? {
                            self.drop_all(PlannerDropPoint::AfterStatement(body), &mut state);
                        }
                    }
                    FunctionForm::Explicit { body, .. } => match body {
                        FunctionBody::Absent => {}
                        FunctionBody::Expression { expression, .. } => {
                            state.forward_scope = self.forward_expression_scope(id, expression);
                            self.enter_forward_region(id, &mut state);
                            if self.expression(
                                expression,
                                DropExpressionUse::Consume,
                                &mut state,
                            )? {
                                let pre_clear = self.forward_region_boundary_clear(&state, true);
                                self.drop_all(
                                    PlannerDropPoint::ControlTransfer(expression),
                                    &mut state,
                                );
                                let post_clear = self.forward_region_boundary_clear(&state, false);
                                self.iteration_actions.complete_forward_region(
                                    state.forward_scope,
                                    pre_clear,
                                    post_clear,
                                );
                            }
                        }
                        FunctionBody::Block(body) => {
                            state.branch_forward_plan = self.branch_forward_plan(id, body);
                            state.nested_forward = self.nested_forward_plan(id, body).map(|plan| {
                                nested_forward_scope::NestedForwardState {
                                    plan,
                                    path: [None; 2],
                                    leaf: None,
                                    ready_split: None,
                                    coverage: None,
                                }
                            });
                            let bounded_coverage =
                                self.begin_bounded_coverage(id, body, &mut state);
                            let coverage = self.begin_branch_coverage(&mut state);
                            let nested_coverage = self.begin_nested_coverage(&mut state);
                            let continues = self.statement(body, &mut state)?;
                            if continues {
                                self.drop_all(PlannerDropPoint::AfterStatement(body), &mut state);
                            }
                            self.finish_branch_coverage(coverage, continues, &state);
                            self.finish_nested_coverage(nested_coverage, continues, &state);
                            self.finish_bounded_coverage(bounded_coverage, continues, &state);
                        }
                    },
                }
                self.finish_owner_observation();
                self.iteration_actions.close_branch_callable(
                    crate::type_checking::UnitItemId::new(self.checker.source_unit, id),
                );
                self.iteration_actions
                    .close_bounded(crate::type_checking::UnitItemId::new(
                        self.checker.source_unit,
                        id,
                    ));
                self.iteration_actions.close_nested_callable(
                    crate::type_checking::UnitItemId::new(self.checker.source_unit, id),
                );
                #[cfg(feature = "d10-catalog-experiment")]
                self.record_callable_drops(
                    super::super::super::d10_catalog_experiment::DropCallable::Function {
                        item: crate::type_checking::UnitItemId::new(self.checker.source_unit, id),
                        has_body,
                    },
                    drop_start,
                );
            }
            Item::Classifier(classifier) => {
                if let Some(body) = classifier.body {
                    for member in body.members {
                        self.item(member)?;
                    }
                }
            }
            Item::Companion(companion) => {
                for member in companion.body.members {
                    self.item(member)?;
                }
            }
            Item::Deinit { body, .. } => {
                #[cfg(feature = "d10-catalog-experiment")]
                let drop_start = (self.facts.len(), self.conditional_receiver_facts.len());
                self.begin_unsupported_owner(
                    crate::ownership_checking::UnitOwnerCallable::Deinit(
                        crate::type_checking::UnitItemId::new(self.checker.source_unit, id),
                    ),
                    PlannerDropPoint::FunctionEntry(id).into_unit(self.checker.source_unit),
                );
                let mut state = ValueState::default();
                if self.statement(body, &mut state)? {
                    self.drop_all(PlannerDropPoint::AfterStatement(body), &mut state);
                }
                self.finish_owner_observation();
                #[cfg(feature = "d10-catalog-experiment")]
                self.record_callable_drops(
                    super::super::super::d10_catalog_experiment::DropCallable::Deinit(
                        crate::type_checking::UnitItemId::new(self.checker.source_unit, id),
                    ),
                    drop_start,
                );
            }
            Item::Error | Item::Variable { .. } | Item::Constant { .. } => {}
        }
        Ok(())
    }
}
