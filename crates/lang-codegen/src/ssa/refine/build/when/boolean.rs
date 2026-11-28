//! Direct branches for exhaustive Boolean when forms.
use super::*;

impl Builder<'_, '_> {
    pub(super) fn boolean_single_arm(
        &self,
        entries: &[WhenEntry],
    ) -> Option<lang_frontend::ast::StatementId> {
        let [only] = entries else {
            return None;
        };
        if only.else_span.is_some() {
            return None;
        }
        let [
            WhenCondition::Expression(left),
            WhenCondition::Expression(right),
        ] = only.conditions.as_slice()
        else {
            return None;
        };
        let (
            Ok((Expression::Literal(lang_frontend::parser::LiteralKind::Boolean(a)), _)),
            Ok((Expression::Literal(lang_frontend::parser::LiteralKind::Boolean(b)), _)),
        ) = (self.node(*left), self.node(*right))
        else {
            return None;
        };
        (a != b).then_some(only.body)
    }

    pub(super) fn lower_boolean_single_arm_when(
        &mut self,
        control: lang_frontend::type_checking::UnitExpressionId,
        ty: UnitTypeId,
        result: Option<Slot>,
        branch: lang_frontend::ast::StatementId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let arm_block = self.body.block();
        let join = self.body.block();
        self.body.blocks[join.0].origin = Some(span);
        self.goto(arm_block, span)?;
        let outer_views = self.active_views.clone();
        let outer_pending_loans = self.pending_call_loans.clone();
        let outer_short_ranges = self.short_ranges.clone();
        let mut predecessors = Vec::new();
        self.active_views = outer_views;
        self.short_ranges = outer_short_ranges.clone();
        let mark = self.enter(arm_block);
        let saved_expected = self.expected_nullable;
        if let Some(res) = result
            && matches!(
                self.inputs.typed.types().get(ty),
                Some(UnitTypeKind::Nullable(_))
            )
            && let EntityType::Value(ssa_ty) = self.body.slots[res.0]
        {
            self.expected_nullable = Some(ssa_ty);
        }
        let value = self.branch_value(branch)?;
        self.expected_nullable = saved_expected;
        self.leave(mark, span)?;
        if let Some(value) = value {
            if let Some(result) = result {
                let (slot, source) =
                    value.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let slot = self.adapt(slot, source, ty)?;
                self.deliver(source);
                self.push(ROp::Alias { source: slot }, vec![result], span)?;
            }
            self.drops(UnitDropPoint::BranchExit { control, branch: 0 })?;
            self.sync_dead();
            if let Some(cur) = self.block {
                predecessors.push(PredecessorBranch {
                    block: cur,
                    views: self.active_views.clone(),
                    dead: self.dead.clone(),
                    this_dead: self.this_dead,
                });
            }
        }
        self.pending_call_loans = outer_pending_loans;
        self.short_ranges = outer_short_ranges;
        let fallthrough = self.collect_drops(UnitDropPoint::BranchExit { control, branch: 1 });
        self.finish_drops(&fallthrough);
        let reached = self.join_views(predecessors, join, span)?;
        if !self.pending_views.is_empty() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        if !reached {
            return Ok(Lowered::Diverged);
        }
        Ok(result.map_or(Lowered::Unit, Lowered::Value))
    }

    pub(super) fn boolean_two_arms(
        &self,
        entries: &[WhenEntry],
    ) -> Option<(
        (usize, lang_frontend::ast::StatementId),
        (usize, lang_frontend::ast::StatementId),
    )> {
        let [first, second] = entries else {
            return None;
        };
        // `true -> ...; else -> ...` is the same closed Boolean decision as
        // true/false arms. Keep its source control identity, not a literal's span.
        if first.else_span.is_none()
            && second.else_span.is_some()
            && second.conditions.is_empty()
            && let [WhenCondition::Expression(condition)] = first.conditions.as_slice()
            && let Ok((Expression::Literal(lang_frontend::parser::LiteralKind::Boolean(value)), _)) =
                self.node(*condition)
        {
            return Some(if value {
                ((0, first.body), (1, second.body))
            } else {
                ((1, second.body), (0, first.body))
            });
        }
        if first.else_span.is_some() || second.else_span.is_some() {
            return None;
        }
        let ([WhenCondition::Expression(left)], [WhenCondition::Expression(right)]) =
            (first.conditions.as_slice(), second.conditions.as_slice())
        else {
            return None;
        };
        let (
            Ok((Expression::Literal(lang_frontend::parser::LiteralKind::Boolean(a)), _)),
            Ok((Expression::Literal(lang_frontend::parser::LiteralKind::Boolean(b)), _)),
        ) = (self.node(*left), self.node(*right))
        else {
            return None;
        };
        match (a, b) {
            (true, false) => Some(((0, first.body), (1, second.body))),
            (false, true) => Some(((1, second.body), (0, first.body))),
            _ => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_boolean_two_arm_when(
        &mut self,
        control: lang_frontend::type_checking::UnitExpressionId,
        ty: UnitTypeId,
        result: Option<Slot>,
        condition: Slot,
        (true_index, true_body): (usize, lang_frontend::ast::StatementId),
        (false_index, false_body): (usize, lang_frontend::ast::StatementId),
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let when_true = self.body.block();
        let when_false = self.body.block();
        let join = self.body.block();
        self.body.blocks[join.0].origin = Some(span);
        self.terminate(
            RTerm::Branch {
                condition,
                when_true,
                when_false,
            },
            span,
        )?;
        let outer_views = self.active_views.clone();
        let outer_pending_loans = self.pending_call_loans.clone();
        let outer_short_ranges = self.short_ranges.clone();
        let outer_temporaries = self.temporaries.clone();
        let mut continuing_temporaries = None;
        let mut predecessors = Vec::new();
        for (block, index, branch) in [
            (when_true, true_index, true_body),
            (when_false, false_index, false_body),
        ] {
            self.active_views = outer_views.clone();
            self.pending_call_loans = outer_pending_loans.clone();
            self.short_ranges = outer_short_ranges.clone();
            self.temporaries.clone_from(&outer_temporaries);
            let mark = self.enter(block);
            let saved_expected = self.expected_nullable;
            if let Some(res) = result
                && matches!(
                    self.inputs.typed.types().get(ty),
                    Some(UnitTypeKind::Nullable(_))
                )
                && let EntityType::Value(ssa_ty) = self.body.slots[res.0]
            {
                self.expected_nullable = Some(ssa_ty);
            }
            let value = self.branch_value(branch)?;
            self.expected_nullable = saved_expected;
            self.leave(mark, span)?;
            let Some(value) = value else {
                continue;
            };
            if let Some(result) = result {
                let (slot, source) =
                    value.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let slot = self.adapt(slot, source, ty)?;
                self.deliver(source);
                self.push(ROp::Alias { source: slot }, vec![result], span)?;
            }
            self.drops(UnitDropPoint::BranchExit {
                control,
                branch: index,
            })?;
            self.sync_dead();
            if let Some(cur) = self.block {
                intersect_temporaries(&mut continuing_temporaries, &self.temporaries);
                predecessors.push(PredecessorBranch {
                    block: cur,
                    views: self.active_views.clone(),
                    dead: self.dead.clone(),
                    this_dead: self.this_dead,
                });
            }
        }
        self.pending_call_loans = outer_pending_loans;
        self.short_ranges = outer_short_ranges;
        self.temporaries = continuing_temporaries.unwrap_or_default();
        let fallthrough = self.collect_drops(UnitDropPoint::BranchExit { control, branch: 2 });
        self.finish_drops(&fallthrough);
        let reached = self.join_views(predecessors, join, span)?;
        if !self.pending_views.is_empty() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        if !reached {
            return Ok(Lowered::Diverged);
        }
        Ok(result.map_or(Lowered::Unit, Lowered::Value))
    }
}
