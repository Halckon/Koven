//! compilation-unit `if` 的 owner-aware CFG 与 branch-state 合流。

mod branch;
mod conditional;
mod nullable_if;
#[cfg(test)]
mod result_tests;

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::{UnitDropPoint, UnitShortCircuitRhs},
    parser::{Expression, LiteralKind, WhenEntry},
    source::Span,
    type_checking::{BuiltinType, Copyability, UnitExpressionId, UnitTypeId, UnitTypeKind},
};

use super::{
    LoweredValue, UnitExpressionLowerer, builtin_type,
    cfg::{BranchExit, carried_control_edge},
    lowering_error, resolve_concrete_type,
    type_lower::is_supported_storage_type,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{
        BlockId, EntityId, EntityType, Operation, Origin, ScalarConstant, SsaTypeId,
        TerminatorKind, ValueId,
    },
};

/// The published control identity and its result representation travel together.
#[derive(Clone, Copy)]
pub(super) struct ControlResult {
    control: UnitExpressionId,
    pub(super) required: bool,
    string: Option<(UnitTypeId, SsaTypeId)>,
}

type BooleanWhenArms = ((StatementId, usize), (StatementId, usize));

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_short_circuit(
        &mut self,
        left: ExpressionId,
        operator: lang_frontend::parser::BinaryOperator,
        right: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let control = UnitExpressionId::new(self.source_unit, expression);
        let plan = self
            .constant_owned
            .map(|owned| {
                owned
                    .short_circuit_at(control)
                    .copied()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
            })
            .transpose()?;
        if let Some(plan) = plan {
            let rhs_branch = match operator {
                lang_frontend::parser::BinaryOperator::LogicalAnd => 0,
                lang_frontend::parser::BinaryOperator::LogicalOr => 1,
                _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            };
            if plan.left() != UnitExpressionId::new(self.source_unit, left)
                || plan.right() != UnitExpressionId::new(self.source_unit, right)
                || plan.rhs_branch() != rhs_branch
            {
                return Err(lowering_error(LoweringErrorKind::MismatchedAnalysis, span));
            }
        }
        // 专用计划只决定左侧正常完成后的 RHS；左侧退出必须直接传播。
        let left = if let Some(plan) = plan {
            let value = self.lower(left)?;
            match value {
                LoweredValue::Diverged => return Ok(value),
                LoweredValue::Unit => {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                LoweredValue::Value(left) => match plan.rhs() {
                    UnitShortCircuitRhs::Never => return Ok(value),
                    UnitShortCircuitRhs::Always => return self.lower(right),
                    UnitShortCircuitRhs::Conditional => left,
                },
            }
        } else {
            let Some(left) = self.lower_optional_condition_value(left, span)? else {
                return Ok(LoweredValue::Diverged);
            };
            left
        };
        if self.typed.expression_falls_through(control) == Some(false) {
            return match self.lower(right)? {
                LoweredValue::Diverged => Ok(LoweredValue::Diverged),
                _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            };
        }
        let baseline = self.bindings.clone();
        let baseline_borrows = self.borrow_bindings.clone();
        let baseline_closures = self.closure_bindings.clone();
        let baseline_frames = self.pending_call_frames.clone();
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

        self.pending_call_frames = baseline_frames.clone();
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
            self.emit_drops(UnitDropPoint::BranchExit {
                control,
                branch: plan.map_or(0, |plan| plan.rhs_branch()),
            })?;
            exits.push(BranchExit {
                block: self.block,
                result: right_result,
                receiver: self.current_receiver,
                consumed_receiver: self.consumed_receiver,
                bindings: self.bindings.clone(),
                borrow_bindings: self.borrow_bindings.clone(),
                nullable_proofs: self.nullable_proofs.clone(),
                closure_bindings: self.closure_bindings.clone(),
                capture_loans: self.capture_loans.clone(),
                result_source_loans: self.result_source_loans.clone(),
                pending_operands: self.pending_operands.clone(),
                pending_call_frames: self.pending_call_frames.clone(),
                temporaries: self.temporaries.clone(),
            });
        }

        self.pending_call_frames = baseline_frames;
        self.block = short_block;
        self.current_receiver = baseline_receiver;
        self.consumed_receiver = baseline_consumed_receiver;
        self.bindings =
            self.rebind_carried_control(&baseline, short_block, &carried, &carried_loans, span)?;
        self.borrow_bindings =
            self.rebind_carried_loans(short_block, carried.len(), &carried_loans, span)?;
        self.closure_bindings = baseline_closures;
        self.emit_drops(UnitDropPoint::BranchExit {
            control,
            branch: plan.map_or(1, |plan| 1 - plan.rhs_branch()),
        })?;
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
            nullable_proofs: self.nullable_proofs.clone(),
            closure_bindings: self.closure_bindings.clone(),
            capture_loans: self.capture_loans.clone(),
            result_source_loans: self.result_source_loans.clone(),
            pending_operands: self.pending_operands.clone(),
            pending_call_frames: self.pending_call_frames.clone(),
            temporaries: self.temporaries.clone(),
        });
        self.merge_unit_exits(exits, span)
    }

    pub(super) fn lower_optional_condition_value(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<ValueId>, LoweringError> {
        match self.lower(expression)? {
            LoweredValue::Value(value) => Ok(Some(value)),
            LoweredValue::Diverged => Ok(None),
            LoweredValue::Unit => {
                let span = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
                    .span();
                Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
            }
        }
    }

    pub(super) fn lower_if(
        &mut self,
        expression: ExpressionId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let result = self.control_result(expression, None, span)?;
        self.lower_conditional(
            expression,
            condition,
            then_branch,
            else_branch,
            0,
            1,
            span,
            result,
        )
    }

    pub(super) fn lower_boolean_when(
        &mut self,
        expression: ExpressionId,
        subject: Option<ExpressionId>,
        entries: &[WhenEntry],
        span: Span,
        expected_string: Option<(UnitTypeId, SsaTypeId)>,
    ) -> Result<LoweredValue, LoweringError> {
        if let Some(subject) = subject
            && self
                .typed
                .expression_falls_through(UnitExpressionId::new(self.source_unit, subject))
                == Some(false)
        {
            return match self.lower_expression(subject)? {
                LoweredValue::Diverged => Ok(LoweredValue::Diverged),
                _ => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
            };
        }
        let control = UnitExpressionId::new(self.source_unit, expression);
        if self.owned.drops().iter().any(|fact| {
            matches!(fact.point(),
            UnitDropPoint::WhenComparisonCompleted { control: owner, .. }
            | UnitDropPoint::WhenAlternativeNoMatch { control: owner, .. } if owner == control)
        }) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if self.typed.nullable_when(control).is_some() {
            if expected_string.is_some() {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            return self.lower_nullable_when(control, span);
        }
        let result = self.control_result(expression, expected_string, span)?;
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
            if matches!(
                self.typed.types().get(subject_type),
                Some(UnitTypeKind::Nullable(_))
            ) {
                return self.missing_nullable_when_descriptor(entries, span);
            }
            if builtin_type(self.typed, subject_type) == Some(BuiltinType::Boolean)
                && let Some((when_true, when_false)) = self.boolean_literal_two_arms(entries)?
            {
                return self.lower_conditional(
                    expression,
                    subject,
                    when_true.0,
                    Some(when_false.0),
                    when_true.1,
                    when_false.1,
                    span,
                    result,
                );
            }
            if matches!(
                self.typed.types().get(subject_type),
                Some(UnitTypeKind::Nominal { .. })
            ) && let Some(lang_frontend::parser::WhenCondition::Expression(cond)) =
                entries.first().and_then(|entry| entry.conditions.first())
                && let Ok(cond_node) = self.parsed.ast().expressions().get(*cond)
            {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    cond_node.span(),
                ));
            }
        }
        Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
    }

    fn boolean_literal_two_arms(
        &self,
        entries: &[WhenEntry],
    ) -> Result<Option<BooleanWhenArms>, LoweringError> {
        let mut when_true = None;
        let mut when_false = None;
        for (index, entry) in entries.iter().enumerate() {
            if entry.else_span.is_some() {
                let arm = (entry.body, index);
                when_true.get_or_insert(arm);
                when_false.get_or_insert(arm);
                break;
            }
            for condition in &entry.conditions {
                let lang_frontend::parser::WhenCondition::Expression(condition) = condition else {
                    return Ok(None);
                };
                let node = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(*condition)
                    .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, entry.span))?;
                match node.payload() {
                    Expression::Literal(LiteralKind::Boolean(true)) => {
                        when_true.get_or_insert((entry.body, index));
                    }
                    Expression::Literal(LiteralKind::Boolean(false)) => {
                        when_false.get_or_insert((entry.body, index));
                    }
                    _ => return Ok(None),
                }
            }
        }
        match (when_true, when_false) {
            (Some(t), Some(f)) if t != f => Ok(Some((t, f))),
            _ => Ok(None),
        }
    }

    pub(super) fn ssa_builtin(
        &self,
        builtin: BuiltinType,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
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

    /// Exact existing String? storage, without widening pointer-only storage recipes.
    fn string_nullable_storage(
        &self,
        ty: lang_frontend::type_checking::UnitTypeId,
    ) -> Option<SsaTypeId> {
        let Some(UnitTypeKind::Nullable(inner)) = self.typed.types().get(ty) else {
            return None;
        };
        if builtin_type(self.typed, *inner) != Some(BuiltinType::String) {
            return None;
        }
        self.type_ids.get(&ty).copied().filter(|storage| {
            matches!(self.ssa_types.get(storage.index()),
                Some(crate::ssa::model::SsaTypeKind::InlineNullable { inner, niche: true })
                if self.ssa_types.get(inner.index()) == Some(&crate::ssa::model::SsaTypeKind::StringOwner))
        })
    }

    pub(super) fn control_result(
        &self,
        expression: ExpressionId,
        expected_string: Option<(UnitTypeId, SsaTypeId)>,
        span: Span,
    ) -> Result<ControlResult, LoweringError> {
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
        let control = UnitExpressionId::new(self.source_unit, expression);
        if let Some((ty, storage)) = expected_string {
            if !matches!(self.typed.types().get(concrete_type), Some(UnitTypeKind::Nullable(inner))
                if builtin_type(self.typed, *inner) == Some(BuiltinType::Nothing))
                || self.string_nullable_storage(ty) != Some(storage)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            return Ok(ControlResult {
                control,
                required: true,
                string: expected_string,
            });
        }
        let result_required = builtin_type(self.typed, concrete_type) != Some(BuiltinType::Unit);
        if result_required
            && builtin_type(self.typed, concrete_type) != Some(BuiltinType::Nothing)
            && !is_supported_storage_type(self.typed, concrete_type)
            && self.string_nullable_storage(concrete_type).is_none()
            && !self
                .type_ids
                .get(&concrete_type)
                .is_some_and(|ty| self.map_results.contains_key(ty))
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(ControlResult {
            control,
            required: result_required,
            string: self
                .string_nullable_storage(concrete_type)
                .map(|storage| (concrete_type, storage)),
        })
    }
}
