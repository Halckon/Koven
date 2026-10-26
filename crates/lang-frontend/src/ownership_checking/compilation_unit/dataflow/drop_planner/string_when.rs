//! Literal-candidate String when: comparison completion is not expression completion.
use super::*;
use crate::parser::{WhenCondition, WhenEntry};

impl DropPlanner<'_, '_> {
    fn ungroup_string_when(
        &self,
        mut id: ExpressionId,
    ) -> Result<ExpressionId, OwnershipCheckingError> {
        while let Expression::Group { expression } =
            self.checker.parsed.ast().expressions().get(id)?.payload()
        {
            id = *expression;
        }
        Ok(id)
    }

    fn string_when_candidates(
        &self,
        subject: ExpressionId,
        entries: &[WhenEntry],
    ) -> Result<Option<Vec<(usize, usize, ExpressionId)>>, OwnershipCheckingError> {
        let subject = self.ungroup_string_when(subject)?;
        let node = self.checker.parsed.ast().expressions().get(subject)?;
        let eligible = match node.payload() {
            Expression::Name => {
                !self.checker.is_constant_use(subject)
                    && self
                        .checker
                        .reference_symbol(node.span())
                        .is_some_and(|symbol| {
                            matches!(
                                self.checker.symbol_kind(symbol),
                                Some(
                                    crate::name_resolution::SymbolKind::Variable
                                        | crate::name_resolution::SymbolKind::ValueParameter
                                        | crate::name_resolution::SymbolKind::LambdaParameter
                                        | crate::name_resolution::SymbolKind::ForBinding
                                        | crate::name_resolution::SymbolKind::DestructuringBinding
                                )
                            )
                        })
            }
            Expression::Member { .. } | Expression::Index { .. } => false,
            _ => self.is_move_only_temporary(subject),
        };
        if !eligible {
            return Ok(None);
        }
        let mut candidates = Vec::new();
        for (entry_index, entry) in entries.iter().enumerate() {
            for (alternative, condition) in entry.conditions.iter().enumerate() {
                let WhenCondition::Expression(expression) = condition else {
                    return Ok(None);
                };
                let candidate = self.ungroup_string_when(*expression)?;
                if !self.is_string_expression(candidate) || !self.is_move_only_temporary(candidate)
                {
                    return Ok(None);
                }
                let Expression::String { parts } = self
                    .checker
                    .parsed
                    .ast()
                    .expressions()
                    .get(candidate)?
                    .payload()
                else {
                    return Ok(None);
                };
                if parts
                    .iter()
                    .any(|part| !matches!(part, StringPart::Text(_)))
                {
                    return Ok(None);
                }
                candidates.push((entry_index, alternative, candidate));
            }
        }
        Ok(Some(candidates))
    }

    pub(super) fn string_when(
        &mut self,
        id: ExpressionId,
        subject: ExpressionId,
        entries: &[WhenEntry],
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let subject = self.ungroup_string_when(subject)?;
        let Some(candidates) = self.string_when_candidates(subject, entries)? else {
            self.resource_deferred.push(UnitOwnershipDeferredFact::new(
                self.checker.unit_expression(id),
                OwnershipDeferredReason::StringWhenComparisonLifetime,
            ));
            return Ok(false);
        };
        let (continues, subject_drop) = self.string_view_operand(subject, state)?;
        if !continues {
            return Ok(false);
        }
        // Required temporary obligations come from the source alternatives, not emit calls.
        let required = self.required_string_when_drops(id, subject_drop, &candidates)?;
        let start = self.facts.len();
        if let Some(StringOperandDrop::Temporary(expression, origin)) = subject_drop {
            self.register_pending_temporary(id, expression, origin, false, state)?;
        }
        if let Some(StringOperandDrop::Named(symbol)) = subject_drop {
            state.pending_borrows.push((id, symbol));
        }
        if candidates.is_empty() {
            let live = entries
                .first()
                .and_then(|_| self.liveness.when_entry_live_in.get(&(id.index(), 0)))
                .cloned()
                .unwrap_or_else(|| self.liveness.expression_after[id.index()].clone());
            self.end_string_when_subject(
                id,
                subject_drop,
                PlannerDropPoint::AfterExpression(subject),
                &live,
                state,
            );
        }
        let usage = self.control_result_usage(id);
        let mut remaining = Some(state.clone());
        let mut exits = Vec::new();
        let last = candidates
            .last()
            .map(|&(entry, alternative, _)| (entry, alternative));
        for (entry_index, entry) in entries.iter().enumerate() {
            let mut matched = Vec::new();
            if entry.else_span.is_some() {
                matched.extend(remaining.take());
            }
            for &(candidate_entry, alternative, candidate) in &candidates {
                if candidate_entry != entry_index {
                    continue;
                }
                let Some(mut input) = remaining.take() else {
                    break;
                };
                if !self.expression(candidate, DropExpressionUse::Read, &mut input)? {
                    break;
                }
                let origin = self
                    .checker
                    .parsed
                    .ast()
                    .expressions()
                    .get(candidate)?
                    .span();
                self.push_fact(PlannerDropFact::new(
                    PlannerDropPoint::WhenComparisonCompleted {
                        control: id,
                        entry: entry_index,
                        alternative,
                    },
                    PlannerDropTarget::Temporary(candidate),
                    origin,
                ));
                let mut selected = input.clone();
                let live = self.liveness.when_entry_live_in[&(id.index(), entry_index)].clone();
                self.end_string_when_subject(
                    id,
                    subject_drop,
                    PlannerDropPoint::WhenAlternativeMatch {
                        control: id,
                        entry: entry_index,
                        alternative,
                    },
                    &live,
                    &mut selected,
                );
                matched.push(selected);
                if last == Some((entry_index, alternative)) {
                    let live = entries
                        .iter()
                        .enumerate()
                        .find(|(_, entry)| entry.else_span.is_some())
                        .and_then(|(index, _)| {
                            self.liveness.when_entry_live_in.get(&(id.index(), index))
                        })
                        .cloned()
                        .unwrap_or_else(|| self.liveness.expression_after[id.index()].clone());
                    self.end_string_when_subject(
                        id,
                        subject_drop,
                        PlannerDropPoint::WhenAlternativeNoMatch {
                            control: id,
                            entry: entry_index,
                            alternative,
                        },
                        &live,
                        &mut input,
                    );
                }
                remaining = Some(input);
            }
            if matched.is_empty() {
                continue;
            }
            let mut branch = self.merge_resource_states(id, matched);
            if self.control_body(entry.body, usage, &mut branch)? {
                self.drop_branch_exit(id, entry_index, &mut branch);
                exits.push(branch);
            }
        }
        if let Some(mut implicit) = remaining {
            self.drop_branch_exit(id, entries.len(), &mut implicit);
            exits.push(implicit);
        }
        // No candidate can diverge in the admitted literal-only kernel. Check that every
        // source-required temporary obligation was emitted exactly once, and no other one.
        let actual: Vec<_> = self.facts[start..]
            .iter()
            .copied()
            .filter(|fact| {
                matches!(fact.target(), PlannerDropTarget::Temporary(_))
                    && match fact.point() {
                        PlannerDropPoint::WhenComparisonCompleted { control, .. }
                        | PlannerDropPoint::WhenAlternativeMatch { control, .. }
                        | PlannerDropPoint::WhenAlternativeNoMatch { control, .. } => control == id,
                        PlannerDropPoint::AfterExpression(expression) => {
                            candidates.is_empty() && expression == subject
                        }
                        _ => false,
                    }
            })
            .collect();
        if actual.len() != required.len()
            || required
                .iter()
                .any(|fact| actual.iter().filter(|actual| *actual == fact).count() != 1)
        {
            return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                source_unit: self.checker.source_unit.index(),
                expression: id.index(),
            });
        }
        let continues = !exits.is_empty();
        *state = self.merge_resource_states(id, exits);
        Ok(continues)
    }

    fn required_string_when_drops(
        &self,
        control: ExpressionId,
        subject: Option<StringOperandDrop>,
        candidates: &[(usize, usize, ExpressionId)],
    ) -> Result<Vec<PlannerDropFact>, OwnershipCheckingError> {
        let mut required = Vec::new();
        for &(entry, alternative, candidate) in candidates {
            required.push(PlannerDropFact::new(
                PlannerDropPoint::WhenComparisonCompleted {
                    control,
                    entry,
                    alternative,
                },
                PlannerDropTarget::Temporary(candidate),
                self.checker
                    .parsed
                    .ast()
                    .expressions()
                    .get(candidate)?
                    .span(),
            ));
            if let Some(StringOperandDrop::Temporary(expression, origin)) = subject {
                required.push(PlannerDropFact::new(
                    PlannerDropPoint::WhenAlternativeMatch {
                        control,
                        entry,
                        alternative,
                    },
                    PlannerDropTarget::Temporary(expression),
                    origin,
                ));
            }
        }
        if let Some(StringOperandDrop::Temporary(expression, origin)) = subject {
            let point = candidates.last().map_or(
                PlannerDropPoint::AfterExpression(expression),
                |&(entry, alternative, _)| PlannerDropPoint::WhenAlternativeNoMatch {
                    control,
                    entry,
                    alternative,
                },
            );
            required.push(PlannerDropFact::new(
                point,
                PlannerDropTarget::Temporary(expression),
                origin,
            ));
        }
        Ok(required)
    }

    fn end_string_when_subject(
        &mut self,
        control: ExpressionId,
        subject: Option<StringOperandDrop>,
        point: PlannerDropPoint,
        live: &std::collections::BTreeSet<UnitSymbolId>,
        state: &mut ValueState,
    ) {
        state
            .pending_borrows
            .retain(|(pending, _)| *pending != control);
        self.finish_pending_temporaries(control, point, state);
        if let Some(StringOperandDrop::Named(symbol)) = subject
            && !live.contains(&symbol)
        {
            self.drop_named_asap(point, symbol, state);
        }
    }
}
