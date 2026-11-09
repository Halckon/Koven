//! `when`: alternatives tested in source order; the first match enters its body. A scalar
//! subject is evaluated once and compared by value or by integer Range membership; a
//! Copyable enum subject is tested by case.
use std::collections::BTreeSet;

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::UnitDropPoint,
    parser::{Expression, WhenCondition, WhenEntry},
    source::Span,
    type_checking::{
        BuiltinType, IntegerRangeOperationKind, UnitExpressionId, UnitTypeId, UnitTypeKind,
    },
};

use super::{Builder, Lowered, View};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{ComparisonOperator, EntityType, SsaTypeId},
    refine::body::{BlockRef, RAbort, ROp, RTerm, Slot},
};

#[derive(Clone, Debug)]
pub(super) struct PredecessorBranch {
    pub(super) block: BlockRef,
    pub(super) views: Vec<View>,
    pub(super) dead: BTreeSet<UnitSymbolId>,
    pub(super) this_dead: bool,
}

#[derive(Clone, Copy)]
enum WhenSubject {
    Value(Slot),
    Tag(Slot),
}

impl Builder<'_, '_> {
    /// Each alternative consumes its `WhenAlternativeMatch` drops on the matched edge;
    /// each body consumes its `BranchExit`, and a statement `when` without `else` consumes
    /// `BranchExit(entries.len())` on the fall-through edge.
    pub(super) fn when_expression(
        &mut self,
        expression: ExpressionId,
        subject: Option<ExpressionId>,
        entries: &[WhenEntry],
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        // The validated subject has no normal exit, so no result or arm plan is needed.
        if let Some(subject) = subject
            && self
                .inputs
                .typed
                .expression_falls_through(self.unit(subject))
                == Some(false)
        {
            return match self.expression(subject)? {
                Lowered::Diverged => Ok(Lowered::Diverged),
                _ => Err(error(LoweringErrorKind::InvalidModel, span)),
            };
        }
        let control = self.unit(expression);
        let ty = self.expression_type(expression)?;
        let result = if self.is_unit(ty) || self.is_nothing(ty) {
            None
        } else {
            let ssa_ty = if matches!(self.inputs.typed.types().get(ty), Some(UnitTypeKind::Nullable(inner)) if self.is_nothing(*inner))
            {
                self.expected_nullable
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            } else {
                self.ssa_type(ty, span)?
            };
            Some(self.body.slot(EntityType::Value(ssa_ty)))
        };
        // A Copyable enum subject is tested by case; its domain is closed.
        let root = match subject {
            Some(subject) => {
                let ty = self.expression_type(subject)?;
                self.is_enum(ty).then_some(ty)
            }
            None => None,
        };
        let string = subject.is_some_and(|subject| {
            self.expression_type(subject).is_ok_and(|ty| {
                matches!(
                    self.inputs.typed.types().get(ty),
                    Some(UnitTypeKind::Builtin(BuiltinType::String))
                )
            })
        });
        let subject_expression = subject
            .map(|subject| {
                if string {
                    self.ungroup_when(subject)
                } else {
                    Ok(subject)
                }
            })
            .transpose()?;
        let subject = match subject_expression {
            Some(subject) => match self.when_subject(subject, string, root)? {
                Some(subject) => Some(subject),
                None => return Ok(Lowered::Diverged),
            },
            None => None,
        };
        if let Some(WhenSubject::Value(subject)) = subject {
            if let Some((true_arm, false_arm)) = self.boolean_two_arms(entries) {
                return self.lower_boolean_two_arm_when(
                    control, ty, result, subject, true_arm, false_arm, span,
                );
            }
            if let Some(body) = self.boolean_single_arm(entries) {
                return self.lower_boolean_single_arm_when(control, ty, result, body, span);
            }
        }
        // One temporary is shared by mutually exclusive outgoing paths. Retain its
        // source identity/slot for the unmatched continuation while constructing a match.
        let subject_temporary = subject_expression.and_then(|expression| {
            let id = self.unit(expression);
            self.temporaries.get(&id).copied().map(|slot| (id, slot))
        });
        let join = self.body.block();
        self.body.blocks[join.0].origin = Some(span);
        let outer_views = self.active_views.clone();
        let outer_pending_loans = self.pending_call_loans.clone();
        let outer_short_ranges = self.short_ranges.clone();
        let mut predecessors = Vec::new();
        let mut falls_through = true;
        let closed_after = self.inputs.typed.when_closed_after(control);
        let reachable_entries = closed_after.unwrap_or(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let exit_point = UnitDropPoint::BranchExit {
                control,
                branch: index,
            };
            if index >= reachable_entries || self.block.is_none() {
                self.unreachable_drop_scopes.push(entry.span);
                self.unreachable_drop_points.push(exit_point);
                for alternative in 0..entry.conditions.len() {
                    self.skip_when_alternative(control, index, alternative);
                }
                continue;
            }
            self.active_views = outer_views.clone();
            self.pending_call_loans = outer_pending_loans.clone();
            self.short_ranges = outer_short_ranges.clone();
            let body = self.body.block();
            if entry.else_span.is_some() {
                falls_through = false;
                self.goto(body, entry.span)?;
            }
            let selector_count = self
                .inputs
                .typed
                .when_final_selector_count(control, index)
                .unwrap_or(entry.conditions.len());
            for (alternative, condition) in entry.conditions.iter().enumerate() {
                if alternative >= selector_count || self.block.is_none() {
                    if let WhenCondition::Expression(expression)
                    | WhenCondition::Contains { expression, .. } = condition
                    {
                        self.unreachable_drop_scopes.push(self.span(*expression));
                    }
                    self.skip_when_alternative(control, index, alternative);
                    continue;
                }
                let matched = self.body.block();
                let next = self.body.block();
                self.when_condition(
                    subject,
                    root,
                    condition,
                    (matched, next),
                    entry.span,
                    string.then_some(UnitDropPoint::WhenComparisonCompleted {
                        control,
                        entry: index,
                        alternative,
                    }),
                )?;
                let match_point = UnitDropPoint::WhenAlternativeMatch {
                    control,
                    entry: index,
                    alternative,
                };
                let miss_point = UnitDropPoint::WhenAlternativeNoMatch {
                    control,
                    entry: index,
                    alternative,
                };
                let matched_reached = self.body.blocks[matched.0].has_predecessor;
                if matched_reached {
                    let mark = self.enter(matched);
                    self.leave_unclaimed(mark, entry.span)?;
                    self.drops(match_point)?;
                    self.goto(body, entry.span)?;
                } else {
                    self.unreachable_drop_points.push(match_point);
                }
                if !self.body.blocks[next.0].has_predecessor {
                    self.unreachable_drop_points.push(miss_point);
                    if !matched_reached {
                        self.unreachable_drop_points
                            .push(UnitDropPoint::WhenComparisonCompleted {
                                control,
                                entry: index,
                                alternative,
                            });
                    }
                    continue;
                }
                let mark = self.enter(next);
                self.leave_unclaimed(mark, entry.span)?;
                if let Some((id, slot)) = subject_temporary {
                    self.temporaries.insert(id, slot);
                }
                self.drops(miss_point)?;
            }
            let unmatched = self.block;
            if unmatched.is_none() {
                falls_through = false;
            }
            if !self.body.blocks[body.0].has_predecessor {
                let span = self.statement_span(entry.body)?;
                self.unreachable_drop_scopes.push(span);
                self.unreachable_drop_points.push(exit_point);
                continue;
            }
            self.active_views = outer_views.clone();
            let mark = self.enter(body);
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
            let value = self.branch_value(entry.body)?;
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
                self.drops(exit_point)?;
                self.sync_dead();
                if let Some(cur) = self.block {
                    predecessors.push(PredecessorBranch {
                        block: cur,
                        views: self.active_views.clone(),
                        dead: self.dead.clone(),
                        this_dead: self.this_dead,
                    });
                }
            } else {
                self.unreachable_drop_points.push(exit_point);
            }
            self.block = unmatched;
        }
        if falls_through
            && (closed_after.is_some()
                || root.is_some()
                    && (result.is_some()
                        || self.inputs.typed.expression_falls_through(control) == Some(false)))
        {
            // The finite subject domain leaves no value on the remaining miss edge.
            let fallthrough = self.collect_drops(UnitDropPoint::BranchExit {
                control,
                branch: entries.len(),
            });
            self.finish_drops(&fallthrough);
            self.terminate(RTerm::Abort(RAbort::UnmatchedWhen), span)?;
        } else if falls_through {
            if result.is_some() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.active_views = outer_views;
            self.drops(UnitDropPoint::BranchExit {
                control,
                branch: entries.len(),
            })?;
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
        if !falls_through {
            self.unreachable_drop_points
                .push(UnitDropPoint::BranchExit {
                    control,
                    branch: entries.len(),
                });
        }
        self.pending_call_loans = outer_pending_loans;
        self.short_ranges = outer_short_ranges;
        let reached = self.join_views(predecessors, join, span)?;
        if !self.pending_views.is_empty() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        if !reached {
            return Ok(Lowered::Diverged);
        }
        Ok(result.map_or(Lowered::Unit, Lowered::Value))
    }

    fn skip_when_alternative(
        &mut self,
        control: UnitExpressionId,
        entry: usize,
        alternative: usize,
    ) {
        self.unreachable_drop_points.extend([
            UnitDropPoint::WhenComparisonCompleted {
                control,
                entry,
                alternative,
            },
            UnitDropPoint::WhenAlternativeMatch {
                control,
                entry,
                alternative,
            },
            UnitDropPoint::WhenAlternativeNoMatch {
                control,
                entry,
                alternative,
            },
        ]);
    }

    fn boolean_single_arm(&self, entries: &[WhenEntry]) -> Option<lang_frontend::ast::StatementId> {
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

    fn lower_boolean_single_arm_when(
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

    fn boolean_two_arms(
        &self,
        entries: &[WhenEntry],
    ) -> Option<(
        (usize, lang_frontend::ast::StatementId),
        (usize, lang_frontend::ast::StatementId),
    )> {
        let [first, second] = entries else {
            return None;
        };
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
    fn lower_boolean_two_arm_when(
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
        let mut predecessors = Vec::new();
        for (block, index, branch) in [
            (when_true, true_index, true_body),
            (when_false, false_index, false_body),
        ] {
            self.active_views = outer_views.clone();
            self.pending_call_loans = outer_pending_loans.clone();
            self.short_ranges = outer_short_ranges.clone();
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

    /// Ends the current block with a branch to `matched` / `next`.
    fn when_condition(
        &mut self,
        subject: Option<WhenSubject>,
        root: Option<UnitTypeId>,
        condition: &WhenCondition,
        edges: (BlockRef, BlockRef),
        span: Span,
        completed: Option<UnitDropPoint>,
    ) -> Result<(), LoweringError> {
        let (matched, next) = edges;
        let (subject, condition) = match (subject, condition) {
            (None, WhenCondition::Expression(condition)) => {
                return self.condition_with_views(*condition, matched, next, false);
            }
            (
                Some(WhenSubject::Value(subject)),
                WhenCondition::Contains {
                    negated,
                    expression,
                    ..
                },
            ) => {
                let (inside, outside) = if *negated {
                    (next, matched)
                } else {
                    (matched, next)
                };
                return self.range_condition(subject, *expression, inside, outside);
            }
            (
                Some(subject),
                WhenCondition::TypeTest {
                    negated, type_ref, ..
                },
            ) => {
                let root = root.ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                let matches = match subject {
                    WhenSubject::Value(owner) => {
                        self.type_test(owner, root, *type_ref, *negated, span)?
                    }
                    WhenSubject::Tag(tag) => {
                        let variant = self.type_test_variant(root, *type_ref, span)?;
                        self.variant_tag_test(tag, variant, *negated, span)?
                    }
                };
                return self.terminate(
                    RTerm::Branch {
                        condition: matches,
                        when_true: matched,
                        when_false: next,
                    },
                    span,
                );
            }
            (Some(WhenSubject::Value(subject)), WhenCondition::Expression(condition))
                if root.is_some() =>
            {
                let root = root.ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                let matches = self.case_value_test(subject, root, *condition, span)?;
                return self.terminate(
                    RTerm::Branch {
                        condition: matches,
                        when_true: matched,
                        when_false: next,
                    },
                    span,
                );
            }
            (Some(WhenSubject::Value(subject)), WhenCondition::Expression(condition)) => {
                (subject, *condition)
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let condition = if completed.is_some() {
            self.ungroup_when(condition)?
        } else {
            condition
        };
        let condition_span = self.span(condition);
        let boolean = self.boolean_type(condition_span)?;
        let Some(candidate) = self.required(condition)? else {
            return Ok(());
        };
        let operation = if completed.is_some() {
            ROp::StringEqual {
                left: subject,
                right: candidate,
            }
        } else {
            ROp::Compare {
                operator: ComparisonOperator::Equal,
                left: subject,
                right: candidate,
            }
        };
        let equal = self.value(operation, EntityType::Value(boolean), condition_span)?;
        if let Some(completed) = completed {
            self.drops(completed)?;
        }
        self.terminate(
            RTerm::Branch {
                condition: equal,
                when_true: matched,
                when_false: next,
            },
            condition_span,
        )
    }

    fn ungroup_when(&self, mut expression: ExpressionId) -> Result<ExpressionId, LoweringError> {
        while let (Expression::Group { expression: inner }, _) = self.node(expression)? {
            expression = inner;
        }
        Ok(expression)
    }

    fn when_subject(
        &mut self,
        expression: ExpressionId,
        string: bool,
        root: Option<UnitTypeId>,
    ) -> Result<Option<WhenSubject>, LoweringError> {
        if string {
            let (payload, span) = self.node(expression)?;
            let id = self.unit(expression);
            if matches!(payload, Expression::Name)
                && self
                    .inputs
                    .constant_owned
                    .is_none_or(|owned| owned.materialization_at(id).is_none())
                && let Some(&symbol) = self.inputs.references.get(&(span.start(), span.end()))
                && let Some(loan) = self.borrowed.get(&symbol).copied()
            {
                // The shared parameter remains owned by the caller. StringEqual can
                // observe its loan directly; never materialize an owned String read.
                self.drops(UnitDropPoint::AfterExpression(id))?;
                return Ok(Some(WhenSubject::Value(loan)));
            }
        }
        if let Some(root_ty) = root
            && self.move_only(root_ty)
        {
            let mut operands = Vec::new();
            let mut current = expression;
            while let (Expression::Group { expression: inner }, _) = self.node(current)? {
                operands.push(current);
                current = inner;
            }
            operands.push(current);
            let (payload, leaf_span) = self.node(current)?;
            if matches!(payload, Expression::Name)
                && let Ok(symbol) = self.reference(leaf_span)
            {
                let snapshot_tag = operands.iter().any(|&op| {
                    let point = UnitDropPoint::AfterExpression(self.unit(op));
                    self.inputs.owned.drops().iter().any(|fact| {
                        fact.point() == point
                            && fact.target()
                                == lang_frontend::ownership_checking::UnitDropTarget::Named(symbol)
                    })
                });
                if snapshot_tag {
                    let Some(owner) = (match self.expression_inner(current)? {
                        Lowered::Value(slot) => Some(slot),
                        Lowered::Diverged => None,
                        Lowered::Unit => {
                            return Err(error(LoweringErrorKind::MissingFact, leaf_span));
                        }
                    }) else {
                        return Ok(None);
                    };
                    let integer = self
                        .inputs
                        .typed
                        .types()
                        .builtin(BuiltinType::Int)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, leaf_span))
                        .and_then(|ty| self.ssa_type(ty, leaf_span))?;
                    let tag = self.value(
                        ROp::TaggedDiscriminant { owner },
                        EntityType::Value(integer),
                        leaf_span,
                    )?;
                    for op in operands.into_iter().rev() {
                        self.register_temporary(op, owner)?;
                        self.drops(UnitDropPoint::AfterExpression(self.unit(op)))?;
                    }
                    return Ok(Some(WhenSubject::Tag(tag)));
                }
            }
        }
        Ok(self.required(expression)?.map(WhenSubject::Value))
    }

    /// `subject in lo..hi` / `lo..<hi`: both endpoints are evaluated in order, then the
    /// subject is tested against the lower and the upper bound.
    fn range_condition(
        &mut self,
        subject: Slot,
        range: ExpressionId,
        inside: BlockRef,
        outside: BlockRef,
    ) -> Result<(), LoweringError> {
        let span = self.span(range);
        let operation = self
            .inputs
            .typed
            .integer_range_operation(self.unit(range))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let upper = match operation.kind() {
            IntegerRangeOperationKind::Inclusive => ComparisonOperator::LessThanOrEqual,
            IntegerRangeOperationKind::Exclusive => ComparisonOperator::LessThan,
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        if operation.left().source_unit() != self.inputs.source_unit
            || operation.right().source_unit() != self.inputs.source_unit
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let Some(lower_bound) = self.required(operation.left().expression())? else {
            return Ok(());
        };
        let Some(upper_bound) = self.required(operation.right().expression())? else {
            return Ok(());
        };
        self.drops(UnitDropPoint::AfterExpression(self.unit(range)))?;
        let boolean = self.boolean_type(span)?;
        let above = self.value(
            ROp::Compare {
                operator: ComparisonOperator::GreaterThanOrEqual,
                left: subject,
                right: lower_bound,
            },
            EntityType::Value(boolean),
            span,
        )?;
        let upper_test = self.body.block();
        self.terminate(
            RTerm::Branch {
                condition: above,
                when_true: upper_test,
                when_false: outside,
            },
            span,
        )?;
        self.block = Some(upper_test);
        let below = self.value(
            ROp::Compare {
                operator: upper,
                left: subject,
                right: upper_bound,
            },
            EntityType::Value(boolean),
            span,
        )?;
        self.terminate(
            RTerm::Branch {
                condition: below,
                when_true: inside,
                when_false: outside,
            },
            span,
        )
    }

    fn boolean_type(&self, span: Span) -> Result<SsaTypeId, LoweringError> {
        let boolean = self
            .inputs
            .typed
            .types()
            .builtin(BuiltinType::Boolean)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.ssa_type(boolean, span)
    }

    /// Joins active views from reached branch predecessors at `join`.
    /// Non-common views in any predecessor end on that predecessor's edge before jumping.
    pub(super) fn join_views(
        &mut self,
        predecessors: Vec<PredecessorBranch>,
        join: BlockRef,
        span: Span,
    ) -> Result<bool, LoweringError> {
        if predecessors.is_empty() {
            self.block = None;
            return Ok(false);
        }
        let mut common = Vec::new();
        if let Some(first) = predecessors.first() {
            for v in &first.views {
                if predecessors[1..]
                    .iter()
                    .all(|other| other.views.iter().any(|ov| ov.slot == v.slot))
                {
                    common.push(*v);
                }
            }
        }
        for pred in predecessors {
            self.block = Some(pred.block);
            self.dead = pred.dead;
            self.this_dead = pred.this_dead;
            self.dead_block = Some(pred.block);
            for v in pred.views.iter().rev() {
                if !common.iter().any(|cv| cv.slot == v.slot) {
                    self.push(ROp::BorrowEnd { loan: v.slot }, Vec::new(), span)?;
                }
            }
            self.goto(join, span)?;
        }
        self.block = Some(join);
        self.active_views = common;
        Ok(true)
    }
}
