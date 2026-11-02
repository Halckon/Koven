//! `when`: alternatives tested in source order; the first match enters its body. A scalar
//! subject is evaluated once and compared by value or by integer Range membership; a
//! Copyable enum subject is tested by case.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    parser::{Expression, WhenCondition, WhenEntry},
    source::Span,
    type_checking::{BuiltinType, IntegerRangeOperationKind, UnitTypeId, UnitTypeKind},
};

use super::{Builder, Lowered, View};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{ComparisonOperator, EntityType, SsaTypeId},
    refine::body::{BlockRef, RAbort, ROp, RTerm, Slot},
};

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
        let control = self.unit(expression);
        let ty = self.expression_type(expression)?;
        let result = if self.is_unit(ty) {
            None
        } else {
            Some(self.body.slot(EntityType::Value(self.ssa_type(ty, span)?)))
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
        if let Some(WhenSubject::Value(subject)) = subject
            && let Some((true_arm, false_arm)) = self.boolean_two_arms(entries)
        {
            return self.lower_boolean_two_arm_when(
                control, ty, result, subject, true_arm, false_arm, span,
            );
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
        let mut predecessors = Vec::new();
        let mut falls_through = true;
        for (index, entry) in entries.iter().enumerate() {
            self.active_views = outer_views.clone();
            self.pending_call_loans = outer_pending_loans.clone();
            let body = self.body.block();
            if entry.else_span.is_some() {
                falls_through = false;
                self.goto(body, entry.span)?;
            }
            for (alternative, condition) in entry.conditions.iter().enumerate() {
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
                let mark = self.enter(matched);
                self.leave_unclaimed(mark, entry.span)?;
                self.drops(UnitDropPoint::WhenAlternativeMatch {
                    control,
                    entry: index,
                    alternative,
                })?;
                self.goto(body, entry.span)?;
                let mark = self.enter(next);
                self.leave_unclaimed(mark, entry.span)?;
                if let Some((id, slot)) = subject_temporary {
                    self.temporaries.insert(id, slot);
                }
                self.drops(UnitDropPoint::WhenAlternativeNoMatch {
                    control,
                    entry: index,
                    alternative,
                })?;
            }
            let unmatched = self.block;
            self.active_views = outer_views.clone();
            let mark = self.enter(body);
            let value = self.branch_value(entry.body)?;
            self.leave(mark, span)?;
            if let Some(value) = value {
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
                if let Some(cur) = self.block {
                    predecessors.push((cur, self.active_views.clone()));
                }
            }
            self.block = unmatched;
            if entry.else_span.is_some() {
                break;
            }
        }
        if falls_through
            && (result.is_some()
                || self.inputs.typed.expression_falls_through(control) == Some(false))
            && root.is_some()
        {
            // The frontend proved the enum cases covered: no value reaches this edge.
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
            if let Some(cur) = self.block {
                predecessors.push((cur, self.active_views.clone()));
            }
        }
        self.pending_call_loans = outer_pending_loans;
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
        let mut predecessors = Vec::new();
        for (block, index, branch) in [
            (when_true, true_index, true_body),
            (when_false, false_index, false_body),
        ] {
            self.active_views = outer_views.clone();
            self.pending_call_loans = outer_pending_loans.clone();
            let mark = self.enter(block);
            let value = self.branch_value(branch)?;
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
            if let Some(cur) = self.block {
                predecessors.push((cur, self.active_views.clone()));
            }
        }
        self.pending_call_loans = outer_pending_loans;
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
        let candidate = self
            .required(condition)?
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, condition_span))?;
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
        let lower_bound = self
            .required(operation.left().expression())?
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let upper_bound = self
            .required(operation.right().expression())?
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
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
        predecessors: Vec<(BlockRef, Vec<View>)>,
        join: BlockRef,
        span: Span,
    ) -> Result<bool, LoweringError> {
        if predecessors.is_empty() {
            self.block = None;
            return Ok(false);
        }
        let mut common = Vec::new();
        if let Some((_, first_views)) = predecessors.first() {
            for v in first_views {
                if predecessors[1..]
                    .iter()
                    .all(|(_, other)| other.iter().any(|ov| ov.slot == v.slot))
                {
                    common.push(*v);
                }
            }
        }
        for (block, views) in predecessors {
            self.block = Some(block);
            for v in views.iter().rev() {
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
