//! compilation-unit `if` 的 owner-aware CFG 与 branch-state 合流。

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::UnitDropPoint,
    parser::{Expression, LiteralKind, TypeRef, WhenCondition, WhenEntry},
    source::Span,
    type_checking::{BuiltinType, Copyability, UnitExpressionId, UnitStatementId},
};

use super::{
    LoweredValue, UnitExpressionLowerer, builtin_type,
    cfg::{BranchExit, carried_control_edge},
    lowering_error, require_value, resolve_concrete_type,
    type_lower::is_supported_storage_type,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{
        BlockId, ComparisonOperator, EntityId, EntityType, Operation, Origin, ScalarConstant,
        SsaTypeId, TerminatorKind, ValueId,
    },
};

#[derive(Clone, Copy)]
struct BooleanWhenArm {
    body: StatementId,
    index: usize,
}

struct BooleanWhenArms {
    when_true: Option<BooleanWhenArm>,
    when_false: Option<BooleanWhenArm>,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_short_circuit(
        &mut self,
        left: ExpressionId,
        operator: lang_frontend::parser::BinaryOperator,
        right: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let left = self.require_expression_value(left)?;
        let baseline = self.bindings.clone();
        let baseline_borrows = self.borrow_bindings.clone();
        let baseline_closures = self.closure_bindings.clone();
        let baseline_receiver = self.current_receiver;
        let baseline_consumed_receiver = self.consumed_receiver;
        let mut carried = self.carried_bindings(&baseline, span)?;
        let mut carried_loans = self.carried_loans(&baseline_borrows, span)?;
        self.carry_pending_operands(&mut carried, &mut carried_loans, span)?;
        let right_span = self
            .parsed
            .ast()
            .expressions()
            .get(right)
            .map(|node| node.span())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let right_block = self.add_carried_control_block(&carried, &carried_loans, right_span)?;
        let short_block = self.add_carried_control_block(&carried, &carried_loans, span)?;
        let (when_true, when_false, short_value) = match operator {
            lang_frontend::parser::BinaryOperator::LogicalAnd => (
                carried_control_edge(right_block, &carried, &carried_loans),
                carried_control_edge(short_block, &carried, &carried_loans),
                false,
            ),
            lang_frontend::parser::BinaryOperator::LogicalOr => (
                carried_control_edge(short_block, &carried, &carried_loans),
                carried_control_edge(right_block, &carried, &carried_loans),
                true,
            ),
            _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
        };
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: left,
                    when_true,
                    when_false,
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        self.block = right_block;
        self.current_receiver = baseline_receiver;
        self.consumed_receiver = baseline_consumed_receiver;
        self.bindings =
            self.rebind_carried_control(&baseline, right_block, &carried, &carried_loans, span)?;
        self.borrow_bindings =
            self.rebind_carried_loans(right_block, carried.len(), &carried_loans, span)?;
        self.closure_bindings = baseline_closures.clone();
        let right_result = self.lower(right)?;
        let mut exits = Vec::with_capacity(2);
        if right_result != LoweredValue::Diverged {
            if !matches!(right_result, LoweredValue::Value(_)) {
                return Err(lowering_error(LoweringErrorKind::MissingFact, right_span));
            }
            exits.push(BranchExit {
                block: self.block,
                result: right_result,
                receiver: self.current_receiver,
                consumed_receiver: self.consumed_receiver,
                bindings: self.bindings.clone(),
                borrow_bindings: self.borrow_bindings.clone(),
                closure_bindings: self.closure_bindings.clone(),
                pending_operands: self.pending_operands.clone(),
                temporaries: self.temporaries.clone(),
            });
        }

        self.block = short_block;
        self.current_receiver = baseline_receiver;
        self.consumed_receiver = baseline_consumed_receiver;
        self.bindings =
            self.rebind_carried_control(&baseline, short_block, &carried, &carried_loans, span)?;
        self.borrow_bindings =
            self.rebind_carried_loans(short_block, carried.len(), &carried_loans, span)?;
        self.closure_bindings = baseline_closures;
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::Constant(ScalarConstant::Boolean(short_value)),
                vec![EntityType::Value(ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Value(short_result) = results[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        exits.push(BranchExit {
            block: self.block,
            result: LoweredValue::Value(short_result),
            receiver: self.current_receiver,
            consumed_receiver: self.consumed_receiver,
            bindings: self.bindings.clone(),
            borrow_bindings: self.borrow_bindings.clone(),
            closure_bindings: self.closure_bindings.clone(),
            pending_operands: self.pending_operands.clone(),
            temporaries: self.temporaries.clone(),
        });
        self.merge_unit_exits(exits, span)
    }

    pub(super) fn lower_if(
        &mut self,
        expression: ExpressionId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.lower_conditional(expression, condition, then_branch, else_branch, 0, 1, span)
    }

    pub(super) fn lower_boolean_when(
        &mut self,
        expression: ExpressionId,
        subject: Option<ExpressionId>,
        entries: &[WhenEntry],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if let Some(subject) = subject {
            let subject_id = UnitExpressionId::new(self.source_unit, subject);
            let subject_type = self
                .typed
                .expression_type(subject_id)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let subject_type = resolve_concrete_type(
                self.typed,
                subject_type,
                self.substitutions,
                self.static_self,
                span,
            )?;
            let subject_ssa = self.expression_ssa_type(subject, span)?;
            let is_boolean = builtin_type(self.typed, subject_type) == Some(BuiltinType::Boolean);
            let is_tagged = self
                .enum_payloads
                .keys()
                .any(|(tagged, _)| *tagged == subject_ssa);
            if !is_boolean && !is_tagged {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            if is_tagged {
                if self.typed.copyability(subject_type) == Copyability::MoveOnly {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                return self.lower_when_chain(
                    expression,
                    Some((subject, Some(subject_ssa))),
                    entries,
                    span,
                );
            }
            if let Some(arms) = self.boolean_literal_arms(entries)? {
                return match (arms.when_true, arms.when_false) {
                    (Some(when_true), Some(when_false))
                        if when_true.body == when_false.body
                            && when_true.index == when_false.index =>
                    {
                        self.lower_exhaustive_boolean_arm(expression, subject, when_true, span)
                    }
                    (Some(when_true), Some(when_false)) => self.lower_conditional(
                        expression,
                        subject,
                        when_true.body,
                        Some(when_false.body),
                        when_true.index,
                        when_false.index,
                        span,
                    ),
                    (Some(when_true), None) => self.lower_conditional(
                        expression,
                        subject,
                        when_true.body,
                        None,
                        when_true.index,
                        entries.len(),
                        span,
                    ),
                    (None, Some(_)) | (None, None) => {
                        self.lower_when_chain(expression, Some((subject, None)), entries, span)
                    }
                };
            }
        }
        self.lower_when_chain(
            expression,
            subject.map(|subject| (subject, None)),
            entries,
            span,
        )
    }

    fn boolean_literal_arms(
        &self,
        entries: &[WhenEntry],
    ) -> Result<Option<BooleanWhenArms>, LoweringError> {
        let mut when_true = None;
        let mut when_false = None;
        for (index, entry) in entries.iter().enumerate() {
            if entry.else_span.is_some() {
                let arm = BooleanWhenArm {
                    body: entry.body,
                    index,
                };
                when_true.get_or_insert(arm);
                when_false.get_or_insert(arm);
                break;
            }
            for condition in &entry.conditions {
                let WhenCondition::Expression(condition) = condition else {
                    return Ok(None);
                };
                let node = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(*condition)
                    .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, entry.span))?;
                let arm = BooleanWhenArm {
                    body: entry.body,
                    index,
                };
                match node.payload() {
                    Expression::Literal(LiteralKind::Boolean(true)) => {
                        when_true.get_or_insert(arm);
                    }
                    Expression::Literal(LiteralKind::Boolean(false)) => {
                        when_false.get_or_insert(arm);
                    }
                    _ => return Ok(None),
                }
            }
        }
        Ok(Some(BooleanWhenArms {
            when_true,
            when_false,
        }))
    }

    fn lower_exhaustive_boolean_arm(
        &mut self,
        expression: ExpressionId,
        subject: ExpressionId,
        arm: BooleanWhenArm,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let result_required = self.control_result_required(expression, span)?;
        self.require_expression_value(subject)?;
        let exit = self.lower_if_branch(
            self.block,
            arm.body,
            self.current_receiver,
            self.consumed_receiver,
            self.bindings.clone(),
            self.borrow_bindings.clone(),
            self.closure_bindings.clone(),
            self.pending_operands.clone(),
            self.temporaries.clone(),
            UnitDropPoint::BranchExit {
                control: UnitExpressionId::new(self.source_unit, expression),
                branch: arm.index,
            },
            result_required,
        )?;
        self.merge_unit_exits(exit.into_iter().collect(), span)
    }

    fn lower_when_chain(
        &mut self,
        expression: ExpressionId,
        subject: Option<(ExpressionId, Option<SsaTypeId>)>,
        entries: &[WhenEntry],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let result_required = self.control_result_required(expression, span)?;
        let subject = match subject {
            Some((subject, tagged)) => Some((self.require_expression_value(subject)?, tagged)),
            None => None,
        };
        let baseline = self.bindings.clone();
        let baseline_borrows = self.borrow_bindings.clone();
        let baseline_closures = self.closure_bindings.clone();
        let control = UnitExpressionId::new(self.source_unit, expression);
        let mut unmatched = Some((
            self.block,
            self.current_receiver,
            self.consumed_receiver,
            baseline.clone(),
            baseline_borrows,
            baseline_closures,
            self.pending_operands.clone(),
            self.temporaries.clone(),
        ));
        let mut exits = Vec::new();

        for (index, entry) in entries.iter().enumerate() {
            let (
                unmatched_block,
                unmatched_receiver,
                unmatched_consumed_receiver,
                unmatched_bindings,
                unmatched_borrows,
                unmatched_closures,
                unmatched_pending,
                unmatched_temporaries,
            ) = unmatched
                .take()
                .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, entry.span))?;
            if entry.else_span.is_some() {
                if let Some(exit) = self.lower_if_branch(
                    unmatched_block,
                    entry.body,
                    unmatched_receiver,
                    unmatched_consumed_receiver,
                    unmatched_bindings,
                    unmatched_borrows,
                    unmatched_closures,
                    unmatched_pending,
                    unmatched_temporaries,
                    UnitDropPoint::BranchExit {
                        control,
                        branch: index,
                    },
                    result_required,
                )? {
                    exits.push(exit);
                }
                break;
            }
            if entry.conditions.is_empty() {
                return Err(lowering_error(LoweringErrorKind::MissingFact, entry.span));
            }

            let mut next_block = unmatched_block;
            let mut next_receiver = unmatched_receiver;
            let mut next_consumed_receiver = unmatched_consumed_receiver;
            let mut next_bindings = unmatched_bindings;
            let mut next_borrows = unmatched_borrows;
            let mut next_closures = unmatched_closures;
            let mut next_pending = unmatched_pending;
            let mut next_temporaries = unmatched_temporaries;
            let mut matches = Vec::new();
            for condition in &entry.conditions {
                self.block = next_block;
                self.current_receiver = next_receiver;
                self.consumed_receiver = next_consumed_receiver;
                self.bindings = next_bindings;
                self.borrow_bindings = next_borrows;
                self.closure_bindings = next_closures;
                self.pending_operands = next_pending;
                let entry_temporaries = next_temporaries.keys().copied().collect::<Vec<_>>();
                self.temporaries = next_temporaries;
                let condition = self.lower_when_condition(subject, condition, entry.span)?;
                if self.temporaries.keys().copied().ne(entry_temporaries) {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        entry.span,
                    ));
                }
                let after_condition = self.bindings.clone();
                let after_condition_borrows = self.borrow_bindings.clone();
                let after_condition_closures = self.closure_bindings.clone();
                let after_condition_receiver = self.current_receiver;
                let after_condition_consumed_receiver = self.consumed_receiver;
                let mut carried = self.carried_bindings(&after_condition, entry.span)?;
                let mut carried_loans = self.carried_loans(&after_condition_borrows, entry.span)?;
                self.carry_pending_operands(&mut carried, &mut carried_loans, entry.span)?;
                let matched =
                    self.add_carried_control_block(&carried, &carried_loans, entry.span)?;
                let next = self.add_carried_control_block(&carried, &carried_loans, entry.span)?;
                self.function
                    .set_terminator(
                        self.block,
                        TerminatorKind::Conditional {
                            condition,
                            when_true: carried_control_edge(matched, &carried, &carried_loans),
                            when_false: carried_control_edge(next, &carried, &carried_loans),
                        },
                        Origin::Source(entry.span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, entry.span))?;
                self.current_receiver = after_condition_receiver;
                self.consumed_receiver = after_condition_consumed_receiver;
                let matched_bindings = self.rebind_carried_control(
                    &after_condition,
                    matched,
                    &carried,
                    &carried_loans,
                    entry.span,
                )?;
                let matched_borrows =
                    self.rebind_carried_loans(matched, carried.len(), &carried_loans, entry.span)?;
                matches.push(BranchExit {
                    block: matched,
                    result: LoweredValue::Unit,
                    bindings: matched_bindings,
                    borrow_bindings: matched_borrows,
                    receiver: self.current_receiver,
                    consumed_receiver: self.consumed_receiver,
                    closure_bindings: after_condition_closures.clone(),
                    pending_operands: self.pending_operands.clone(),
                    temporaries: self.temporaries.clone(),
                });
                next_block = next;
                self.current_receiver = after_condition_receiver;
                self.consumed_receiver = after_condition_consumed_receiver;
                next_bindings = self.rebind_carried_control(
                    &after_condition,
                    next,
                    &carried,
                    &carried_loans,
                    entry.span,
                )?;
                next_borrows =
                    self.rebind_carried_loans(next, carried.len(), &carried_loans, entry.span)?;
                next_receiver = self.current_receiver;
                next_consumed_receiver = self.consumed_receiver;
                next_closures = after_condition_closures;
                next_pending = self.pending_operands.clone();
                next_temporaries = self.temporaries.clone();
            }

            self.merge_unit_exits(matches, entry.span)?;
            if let Some(exit) = self.lower_if_branch(
                self.block,
                entry.body,
                self.current_receiver,
                self.consumed_receiver,
                self.bindings.clone(),
                self.borrow_bindings.clone(),
                self.closure_bindings.clone(),
                self.pending_operands.clone(),
                self.temporaries.clone(),
                UnitDropPoint::BranchExit {
                    control,
                    branch: index,
                },
                result_required,
            )? {
                exits.push(exit);
            }
            unmatched = Some((
                next_block,
                next_receiver,
                next_consumed_receiver,
                next_bindings,
                next_borrows,
                next_closures,
                next_pending,
                next_temporaries,
            ));
        }

        if let Some((
            unmatched_block,
            unmatched_receiver,
            unmatched_consumed_receiver,
            unmatched_bindings,
            unmatched_borrows,
            unmatched_closures,
            unmatched_pending,
            unmatched_temporaries,
        )) = unmatched
        {
            if result_required {
                if subject.is_some_and(|(_, tagged)| tagged.is_some()) {
                    self.function
                        .set_terminator(
                            unmatched_block,
                            TerminatorKind::Abort,
                            Origin::Source(span),
                        )
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                } else {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                return self.merge_unit_exits(exits, span);
            }
            self.block = unmatched_block;
            self.current_receiver = unmatched_receiver;
            self.consumed_receiver = unmatched_consumed_receiver;
            self.bindings = unmatched_bindings;
            self.borrow_bindings = unmatched_borrows;
            self.closure_bindings = unmatched_closures;
            self.pending_operands = unmatched_pending;
            self.temporaries = unmatched_temporaries;
            self.emit_drops(UnitDropPoint::BranchExit {
                control,
                branch: entries.len(),
            })?;
            exits.push(BranchExit {
                block: self.block,
                result: LoweredValue::Unit,
                receiver: self.current_receiver,
                consumed_receiver: self.consumed_receiver,
                bindings: self.bindings.clone(),
                borrow_bindings: self.borrow_bindings.clone(),
                closure_bindings: self.closure_bindings.clone(),
                pending_operands: self.pending_operands.clone(),
                temporaries: self.temporaries.clone(),
            });
        }
        self.merge_unit_exits(exits, span)
    }

    fn lower_when_condition(
        &mut self,
        subject: Option<(ValueId, Option<SsaTypeId>)>,
        condition: &WhenCondition,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        if let WhenCondition::TypeTest {
            negated, type_ref, ..
        } = condition
        {
            let Some((owner, Some(tagged))) = subject else {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            };
            let node = self
                .parsed
                .ast()
                .type_refs()
                .get(*type_ref)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let TypeRef::Qualified { segments, .. } = node.payload() else {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    node.span(),
                ));
            };
            let symbol = segments
                .last()
                .and_then(|segment| {
                    self.type_references
                        .get(&super::span_key(segment.name_span))
                })
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, node.span()))?;
            let case = self
                .typed
                .signatures()
                .declarations()
                .iter()
                .filter_map(|signature| signature.nominal())
                .flat_map(|nominal| nominal.enum_cases())
                .find(|case| case.type_symbol() == symbol)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, node.span()))?;
            let (variant, _) = self
                .enum_payloads
                .get(&(tagged, case.type_symbol()))
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, node.span()))?;
            let integer = self.ssa_builtin(BuiltinType::Int, span)?;
            let boolean = self.ssa_builtin(BuiltinType::Boolean, span)?;
            let (_, tag) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::TaggedDiscriminant { owner },
                    vec![EntityType::Value(integer)],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            let (_, expected) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::Constant(ScalarConstant::Integer(
                        i128::try_from(variant)
                            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?,
                    )),
                    vec![EntityType::Value(integer)],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            let (_, matches) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::Compare {
                        operator: ComparisonOperator::Equal,
                        left: require_value(tag[0], span)?,
                        right: require_value(expected[0], span)?,
                    },
                    vec![EntityType::Value(boolean)],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            let matches = require_value(matches[0], span)?;
            if !negated {
                return Ok(matches);
            }
            let (_, results) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::BooleanNot { operand: matches },
                    vec![EntityType::Value(boolean)],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            return require_value(results[0], span);
        }
        let WhenCondition::Expression(expression) = condition else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if subject.is_some_and(|(_, tagged)| tagged.is_some()) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let candidate = self.require_expression_value(*expression)?;
        let Some((subject, None)) = subject else {
            return Ok(candidate);
        };
        let boolean = self.expression_ssa_type(*expression, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::Compare {
                    operator: ComparisonOperator::Equal,
                    left: subject,
                    right: candidate,
                },
                vec![EntityType::Value(boolean)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Value(result) = results[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        Ok(result)
    }

    fn ssa_builtin(&self, builtin: BuiltinType, span: Span) -> Result<SsaTypeId, LoweringError> {
        let ty = self
            .typed
            .types()
            .builtin(builtin)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.type_ids
            .get(&ty)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    fn control_result_required(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let expression_type = self
            .typed
            .expression_type(UnitExpressionId::new(self.source_unit, expression))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let concrete_type = resolve_concrete_type(
            self.typed,
            expression_type,
            self.substitutions,
            self.static_self,
            span,
        )?;
        let result_required = builtin_type(self.typed, concrete_type) != Some(BuiltinType::Unit);
        if result_required
            && builtin_type(self.typed, concrete_type) != Some(BuiltinType::Nothing)
            && !is_supported_storage_type(self.typed, concrete_type)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(result_required)
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_conditional(
        &mut self,
        expression: ExpressionId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        then_index: usize,
        else_index: usize,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let control = UnitExpressionId::new(self.source_unit, expression);
        let result_required = self.control_result_required(expression, span)?;
        let condition = self.require_expression_value(condition)?;
        let baseline = self.bindings.clone();
        let baseline_borrows = self.borrow_bindings.clone();
        let baseline_closures = self.closure_bindings.clone();
        let baseline_receiver = self.current_receiver;
        let baseline_consumed_receiver = self.consumed_receiver;
        let mut carried = self.carried_bindings(&baseline, span)?;
        let mut carried_loans = self.carried_loans(&baseline_borrows, span)?;
        self.carry_pending_operands(&mut carried, &mut carried_loans, span)?;
        let then_block = self.add_carried_control_block(
            &carried,
            &carried_loans,
            self.statement_span(then_branch)?,
        )?;
        let else_origin = else_branch
            .map(|branch| self.statement_span(branch))
            .transpose()?
            .unwrap_or(span);
        let else_block = self.add_carried_control_block(&carried, &carried_loans, else_origin)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition,
                    when_true: carried_control_edge(then_block, &carried, &carried_loans),
                    when_false: carried_control_edge(else_block, &carried, &carried_loans),
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        self.current_receiver = baseline_receiver;
        self.consumed_receiver = baseline_consumed_receiver;
        let then_baseline =
            self.rebind_carried_control(&baseline, then_block, &carried, &carried_loans, span)?;
        let then_borrows =
            self.rebind_carried_loans(then_block, carried.len(), &carried_loans, span)?;
        let then_pending = self.pending_operands.clone();
        let then_temporaries = self.temporaries.clone();
        let then_receiver = self.current_receiver;
        let then_consumed_receiver = self.consumed_receiver;
        self.current_receiver = baseline_receiver;
        self.consumed_receiver = baseline_consumed_receiver;
        let else_baseline =
            self.rebind_carried_control(&baseline, else_block, &carried, &carried_loans, span)?;
        let else_borrows =
            self.rebind_carried_loans(else_block, carried.len(), &carried_loans, span)?;
        let else_pending = self.pending_operands.clone();
        let else_temporaries = self.temporaries.clone();
        let else_receiver = self.current_receiver;
        let else_consumed_receiver = self.consumed_receiver;
        let mut exits = Vec::with_capacity(2);
        if let Some(exit) = self.lower_if_branch(
            then_block,
            then_branch,
            then_receiver,
            then_consumed_receiver,
            then_baseline,
            then_borrows,
            baseline_closures.clone(),
            then_pending,
            then_temporaries,
            UnitDropPoint::BranchExit {
                control,
                branch: then_index,
            },
            result_required,
        )? {
            exits.push(exit);
        }
        if let Some(else_branch) = else_branch {
            if let Some(exit) = self.lower_if_branch(
                else_block,
                else_branch,
                else_receiver,
                else_consumed_receiver,
                else_baseline,
                else_borrows,
                baseline_closures.clone(),
                else_pending,
                else_temporaries,
                UnitDropPoint::BranchExit {
                    control,
                    branch: else_index,
                },
                result_required,
            )? {
                exits.push(exit);
            }
        } else {
            if result_required {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            self.block = else_block;
            self.current_receiver = else_receiver;
            self.consumed_receiver = else_consumed_receiver;
            self.bindings = else_baseline;
            self.borrow_bindings = else_borrows;
            self.closure_bindings = baseline_closures;
            self.pending_operands = else_pending;
            self.temporaries = else_temporaries;
            self.emit_drops(UnitDropPoint::BranchExit {
                control,
                branch: else_index,
            })?;
            exits.push(BranchExit {
                block: else_block,
                result: LoweredValue::Unit,
                receiver: self.current_receiver,
                consumed_receiver: self.consumed_receiver,
                bindings: self.bindings.clone(),
                borrow_bindings: self.borrow_bindings.clone(),
                closure_bindings: self.closure_bindings.clone(),
                pending_operands: self.pending_operands.clone(),
                temporaries: self.temporaries.clone(),
            });
        }
        self.merge_unit_exits(exits, span)
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_if_branch(
        &mut self,
        block: BlockId,
        statement: StatementId,
        receiver: Option<super::ReceiverBinding>,
        consumed_receiver: Option<super::ConsumedReceiver>,
        bindings: BTreeMap<UnitSymbolId, LoweredValue>,
        borrow_bindings: BTreeMap<UnitSymbolId, crate::ssa::model::LoanId>,
        closure_bindings: BTreeMap<UnitSymbolId, UnitExpressionId>,
        pending_operands: Vec<EntityId>,
        temporaries: BTreeMap<UnitExpressionId, ValueId>,
        drop_point: UnitDropPoint,
        result_required: bool,
    ) -> Result<Option<BranchExit>, LoweringError> {
        self.block = block;
        self.current_receiver = receiver;
        self.consumed_receiver = consumed_receiver;
        let entry_symbols = bindings.keys().copied().collect();
        self.bindings = bindings;
        self.borrow_bindings = borrow_bindings;
        self.closure_bindings = closure_bindings;
        self.pending_operands = pending_operands;
        let entry_temporaries = temporaries.keys().copied().collect::<Vec<_>>();
        self.temporaries = temporaries;
        let result_expression = result_required
            .then(|| self.control_tail_expression(statement))
            .transpose()?;
        let result = if result_required {
            self.lower_tail_value_body(statement)?
        } else {
            self.lower_statement(statement)?
        };
        if result == LoweredValue::Diverged {
            return Ok(None);
        }
        if let (Some(expression), LoweredValue::Value(value)) = (result_expression, result) {
            self.transfer_owned_expression(expression, value, self.statement_span(statement)?)?;
        }
        let expected_result = if result_required {
            matches!(result, LoweredValue::Value(_))
        } else {
            result == LoweredValue::Unit
        };
        if !expected_result || self.temporaries.keys().copied().ne(entry_temporaries) {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                self.statement_span(statement)?,
            ));
        }
        self.emit_drops(drop_point)?;
        self.discard_non_entry_bindings(&entry_symbols, self.statement_span(statement)?)?;
        Ok(Some(BranchExit {
            block: self.block,
            result,
            receiver: self.current_receiver,
            consumed_receiver: self.consumed_receiver,
            bindings: self.bindings.clone(),
            borrow_bindings: self.borrow_bindings.clone(),
            closure_bindings: self.closure_bindings.clone(),
            pending_operands: self.pending_operands.clone(),
            temporaries: self.temporaries.clone(),
        }))
    }

    fn control_tail_expression(
        &self,
        statement: StatementId,
    ) -> Result<ExpressionId, LoweringError> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let tail = match node.payload() {
            lang_frontend::parser::Statement::Expression { expression } => return Ok(*expression),
            lang_frontend::parser::Statement::ControlBody { elements } => elements.last(),
            _ => None,
        }
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, node.span()))?;
        let tail = self
            .parsed
            .ast()
            .statements()
            .get(*tail)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, node.span()))?;
        match tail.payload() {
            lang_frontend::parser::Statement::Expression { expression } => Ok(*expression),
            _ => Err(lowering_error(LoweringErrorKind::MissingFact, tail.span())),
        }
    }

    pub(super) fn lower_tail_value_body(
        &mut self,
        statement: StatementId,
    ) -> Result<LoweredValue, LoweringError> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let elements = match node.payload() {
            lang_frontend::parser::Statement::ControlBody { elements }
            | lang_frontend::parser::Statement::LambdaBody { elements } => elements,
            _ => return self.lower_statement(statement),
        };
        let elements = elements.clone();
        let Some((&last, prefix)) = elements.split_last() else {
            return Ok(LoweredValue::Unit);
        };
        for &element in prefix {
            if self.lower_statement(element)? == LoweredValue::Diverged {
                return Ok(LoweredValue::Diverged);
            }
        }
        let result = self.lower_statement(last)?;
        if result != LoweredValue::Diverged {
            self.emit_drops(UnitDropPoint::AfterStatement(UnitStatementId::new(
                self.source_unit,
                statement,
            )))?;
        }
        Ok(result)
    }

    pub(super) fn statement_span(&self, statement: StatementId) -> Result<Span, LoweringError> {
        self.parsed
            .ast()
            .statements()
            .get(statement)
            .map(|statement| statement.span())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })
    }
}
