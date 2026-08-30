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
    LoweredValue, UnitExpressionLowerer, builtin_type, cfg::carried_edge, lowering_error,
    require_value, resolve_concrete_type,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{
        BlockId, ComparisonOperator, Edge, EntityId, EntityType, Operation, Origin, ScalarConstant,
        SsaTypeId, TerminatorKind, ValueId,
    },
};

pub(super) struct BranchExit {
    pub(super) block: BlockId,
    pub(super) result: LoweredValue,
    pub(super) bindings: BTreeMap<UnitSymbolId, LoweredValue>,
    pub(super) closure_bindings: BTreeMap<UnitSymbolId, UnitExpressionId>,
}

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
        let baseline_closures = self.closure_bindings.clone();
        let carried = self.carried_bindings(&baseline, span)?;
        let right_span = self
            .parsed
            .ast()
            .expressions()
            .get(right)
            .map(|node| node.span())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let right_block = self.add_carried_block(&carried, right_span)?;
        let short_block = self.add_carried_block(&carried, span)?;
        let (when_true, when_false, short_value) = match operator {
            lang_frontend::parser::BinaryOperator::LogicalAnd => (
                carried_edge(right_block, &carried),
                carried_edge(short_block, &carried),
                false,
            ),
            lang_frontend::parser::BinaryOperator::LogicalOr => (
                carried_edge(short_block, &carried),
                carried_edge(right_block, &carried),
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
        self.bindings = self.rebind_carried(&baseline, right_block, &carried, span)?;
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
                bindings: self.bindings.clone(),
                closure_bindings: self.closure_bindings.clone(),
            });
        }

        self.block = short_block;
        self.bindings = self.rebind_carried(&baseline, short_block, &carried, span)?;
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
            bindings: self.bindings.clone(),
            closure_bindings: self.closure_bindings.clone(),
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
                .types()
                .expression_type(subject_id)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let subject_type =
                resolve_concrete_type(self.typed, subject_type, self.substitutions, span)?;
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
                if self.typed.types().copyability(subject_type) == Copyability::MoveOnly {
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
        if !self.temporaries.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let result_required = self.control_result_required(expression, span)?;
        self.require_expression_value(subject)?;
        let exit = self.lower_if_branch(
            self.block,
            arm.body,
            self.bindings.clone(),
            self.closure_bindings.clone(),
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
        if !self.temporaries.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let result_required = self.control_result_required(expression, span)?;
        let subject = match subject {
            Some((subject, tagged)) => Some((self.require_expression_value(subject)?, tagged)),
            None => None,
        };
        let baseline = self.bindings.clone();
        let baseline_closures = self.closure_bindings.clone();
        let control = UnitExpressionId::new(self.source_unit, expression);
        let mut unmatched = Some((self.block, baseline.clone(), baseline_closures));
        let mut exits = Vec::new();

        for (index, entry) in entries.iter().enumerate() {
            let (unmatched_block, unmatched_bindings, unmatched_closures) = unmatched
                .take()
                .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, entry.span))?;
            if entry.else_span.is_some() {
                if let Some(exit) = self.lower_if_branch(
                    unmatched_block,
                    entry.body,
                    unmatched_bindings,
                    unmatched_closures,
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
            let mut next_bindings = unmatched_bindings;
            let mut next_closures = unmatched_closures;
            let mut matches = Vec::new();
            for condition in &entry.conditions {
                self.block = next_block;
                self.bindings = next_bindings;
                self.closure_bindings = next_closures;
                self.temporaries.clear();
                let condition = self.lower_when_condition(subject, condition, entry.span)?;
                if !self.temporaries.is_empty() {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        entry.span,
                    ));
                }
                let after_condition = self.bindings.clone();
                let after_condition_closures = self.closure_bindings.clone();
                let carried = self.carried_bindings(&after_condition, entry.span)?;
                let matched = self.add_carried_block(&carried, entry.span)?;
                let next = self.add_carried_block(&carried, entry.span)?;
                self.function
                    .set_terminator(
                        self.block,
                        TerminatorKind::Conditional {
                            condition,
                            when_true: carried_edge(matched, &carried),
                            when_false: carried_edge(next, &carried),
                        },
                        Origin::Source(entry.span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, entry.span))?;
                matches.push(BranchExit {
                    block: matched,
                    result: LoweredValue::Unit,
                    bindings: self.rebind_carried(
                        &after_condition,
                        matched,
                        &carried,
                        entry.span,
                    )?,
                    closure_bindings: after_condition_closures.clone(),
                });
                next_block = next;
                next_bindings =
                    self.rebind_carried(&after_condition, next, &carried, entry.span)?;
                next_closures = after_condition_closures;
            }

            self.merge_unit_exits(matches, entry.span)?;
            if let Some(exit) = self.lower_if_branch(
                self.block,
                entry.body,
                self.bindings.clone(),
                self.closure_bindings.clone(),
                UnitDropPoint::BranchExit {
                    control,
                    branch: index,
                },
                result_required,
            )? {
                exits.push(exit);
            }
            unmatched = Some((next_block, next_bindings, next_closures));
        }

        if let Some((unmatched_block, unmatched_bindings, unmatched_closures)) = unmatched {
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
            self.bindings = unmatched_bindings;
            self.closure_bindings = unmatched_closures;
            self.temporaries.clear();
            self.emit_drops(UnitDropPoint::BranchExit {
                control,
                branch: entries.len(),
            })?;
            exits.push(BranchExit {
                block: self.block,
                result: LoweredValue::Unit,
                bindings: self.bindings.clone(),
                closure_bindings: self.closure_bindings.clone(),
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
                .types()
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
            .types()
            .expression_type(UnitExpressionId::new(self.source_unit, expression))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let concrete_type =
            resolve_concrete_type(self.typed, expression_type, self.substitutions, span)?;
        let result_required = builtin_type(self.typed, concrete_type) != Some(BuiltinType::Unit);
        if result_required
            && (builtin_type(self.typed, concrete_type).is_none()
                || self.typed.types().copyability(concrete_type) != Copyability::Copyable)
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
        if !self.temporaries.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let result_required = self.control_result_required(expression, span)?;
        let condition = self.require_expression_value(condition)?;
        let baseline = self.bindings.clone();
        let baseline_closures = self.closure_bindings.clone();
        let carried = self.carried_bindings(&baseline, span)?;
        let then_block = self.add_carried_block(&carried, self.statement_span(then_branch)?)?;
        let else_origin = else_branch
            .map(|branch| self.statement_span(branch))
            .transpose()?
            .unwrap_or(span);
        let else_block = self.add_carried_block(&carried, else_origin)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition,
                    when_true: carried_edge(then_block, &carried),
                    when_false: carried_edge(else_block, &carried),
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        let then_baseline = self.rebind_carried(&baseline, then_block, &carried, span)?;
        let else_baseline = self.rebind_carried(&baseline, else_block, &carried, span)?;
        let mut exits = Vec::with_capacity(2);
        if let Some(exit) = self.lower_if_branch(
            then_block,
            then_branch,
            then_baseline,
            baseline_closures.clone(),
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
                else_baseline,
                baseline_closures.clone(),
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
            self.bindings = else_baseline;
            self.closure_bindings = baseline_closures;
            self.temporaries.clear();
            self.emit_drops(UnitDropPoint::BranchExit {
                control,
                branch: else_index,
            })?;
            exits.push(BranchExit {
                block: else_block,
                result: LoweredValue::Unit,
                bindings: self.bindings.clone(),
                closure_bindings: self.closure_bindings.clone(),
            });
        }
        self.merge_unit_exits(exits, span)
    }

    fn lower_if_branch(
        &mut self,
        block: BlockId,
        statement: StatementId,
        bindings: BTreeMap<UnitSymbolId, LoweredValue>,
        closure_bindings: BTreeMap<UnitSymbolId, UnitExpressionId>,
        drop_point: UnitDropPoint,
        result_required: bool,
    ) -> Result<Option<BranchExit>, LoweringError> {
        self.block = block;
        let entry_symbols = bindings.keys().copied().collect();
        self.bindings = bindings;
        self.closure_bindings = closure_bindings;
        self.temporaries.clear();
        let result = if result_required {
            self.lower_control_body(statement)?
        } else {
            self.lower_statement(statement)?
        };
        if result == LoweredValue::Diverged {
            return Ok(None);
        }
        let expected_result = if result_required {
            matches!(result, LoweredValue::Value(_))
        } else {
            result == LoweredValue::Unit
        };
        if !expected_result || !self.temporaries.is_empty() {
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
            bindings: self.bindings.clone(),
            closure_bindings: self.closure_bindings.clone(),
        }))
    }

    fn lower_control_body(
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
        let lang_frontend::parser::Statement::ControlBody { elements } = node.payload() else {
            return self.lower_statement(statement);
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

    pub(super) fn merge_unit_exits(
        &mut self,
        exits: Vec<BranchExit>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let Some(first) = exits.first() else {
            self.bindings.clear();
            self.closure_bindings.clear();
            self.temporaries.clear();
            return Ok(LoweredValue::Diverged);
        };
        if exits.len() == 1 {
            self.block = first.block;
            self.bindings = first.bindings.clone();
            self.closure_bindings = first.closure_bindings.clone();
            self.temporaries.clear();
            return Ok(first.result);
        }
        let symbols = first.bindings.keys().copied().collect::<Vec<_>>();
        if exits.iter().any(|exit| {
            exit.bindings.keys().copied().collect::<Vec<_>>() != symbols
                || exit.closure_bindings != first.closure_bindings
                || std::mem::discriminant(&exit.result) != std::mem::discriminant(&first.result)
                || symbols.iter().any(|symbol| {
                    std::mem::discriminant(&exit.bindings[symbol])
                        != std::mem::discriminant(&first.bindings[symbol])
                })
        }) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let result_type = match first.result {
            LoweredValue::Unit => None,
            LoweredValue::Value(value) => Some(
                self.function
                    .entity(EntityId::Value(value))
                    .map(|entity| entity.ty)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?,
            ),
            LoweredValue::Diverged => {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            }
        };
        if let Some(expected) = result_type
            && exits.iter().any(|exit| match exit.result {
                LoweredValue::Value(value) => self
                    .function
                    .entity(EntityId::Value(value))
                    .is_none_or(|entity| entity.ty != expected),
                LoweredValue::Unit | LoweredValue::Diverged => true,
            })
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let value_symbols = symbols
            .iter()
            .copied()
            .filter(|symbol| matches!(first.bindings[symbol], LoweredValue::Value(_)))
            .collect::<Vec<_>>();
        let mut parameter_types = result_type.into_iter().collect::<Vec<_>>();
        parameter_types.extend(
            value_symbols
                .iter()
                .map(|symbol| match first.bindings[symbol] {
                    LoweredValue::Value(value) => self
                        .function
                        .entity(EntityId::Value(value))
                        .map(|entity| entity.ty)
                        .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span)),
                    LoweredValue::Unit | LoweredValue::Diverged => {
                        Err(lowering_error(LoweringErrorKind::InvalidModel, span))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
        let merge = self
            .function
            .add_block(parameter_types, Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        for exit in &exits {
            let mut arguments = match exit.result {
                LoweredValue::Unit => Vec::new(),
                LoweredValue::Value(value) => vec![EntityId::Value(value)],
                LoweredValue::Diverged => {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                }
            };
            arguments.extend(
                value_symbols
                    .iter()
                    .map(|symbol| match exit.bindings[symbol] {
                        LoweredValue::Value(value) => Ok(EntityId::Value(value)),
                        LoweredValue::Unit | LoweredValue::Diverged => {
                            Err(lowering_error(LoweringErrorKind::InvalidModel, span))
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
            self.function
                .set_terminator(
                    exit.block,
                    TerminatorKind::Branch(Edge {
                        target: merge,
                        arguments,
                    }),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        }
        let parameters = self
            .function
            .block(merge)
            .expect("new merge block exists")
            .parameters
            .clone();
        let mut parameters = parameters.into_iter();
        let result = if result_type.is_some() {
            let EntityId::Value(value) = parameters
                .next()
                .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            LoweredValue::Value(value)
        } else {
            LoweredValue::Unit
        };
        let mut bindings = first.bindings.clone();
        for (symbol, parameter) in value_symbols.into_iter().zip(parameters) {
            let EntityId::Value(value) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            bindings.insert(symbol, LoweredValue::Value(value));
        }
        self.block = merge;
        self.bindings = bindings;
        self.closure_bindings = first.closure_bindings.clone();
        self.temporaries.clear();
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
