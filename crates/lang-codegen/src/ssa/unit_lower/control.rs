//! compilation-unit `if` 的 owner-aware CFG 与 branch-state 合流。

mod conditional;
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
        let ordinary_edges = plan.is_none().then(|| {
            if self.typed.ordinary_rhs_bypassed(control) {
                return (false, true);
            }
            if self.typed.ordinary_rhs_required(control) {
                return (true, false);
            }
            let (yes, no) = self
                .typed
                .ordinary_boolean_outcomes(UnitExpressionId::new(self.source_unit, left));
            match operator {
                lang_frontend::parser::BinaryOperator::LogicalAnd => (yes, no),
                lang_frontend::parser::BinaryOperator::LogicalOr => (no, yes),
                _ => (true, true),
            }
        });
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
        match ordinary_edges {
            Some((false, true)) => {
                self.emit_drops(UnitDropPoint::BranchExit { control, branch: 1 })?;
                return Ok(LoweredValue::Value(left));
            }
            Some((true, false)) => {
                let rhs = self.lower(right)?;
                match rhs {
                    LoweredValue::Value(_) => {
                        self.emit_drops(UnitDropPoint::BranchExit { control, branch: 0 })?;
                    }
                    LoweredValue::Diverged => {}
                    LoweredValue::Unit => {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                }
                return Ok(rhs);
            }
            _ => {}
        }
        let baseline = self.bindings.clone();
        let baseline_borrows = self.borrow_bindings.clone();
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

pub(in crate::ssa::unit_lower) mod scalar {
    use std::collections::BTreeMap;

    use lang_frontend::{
        ast::ExpressionId,
        name_resolution::UnitSymbolId,
        ownership_checking::UnitDropPoint,
        parser::{BinaryOperator, Expression, LiteralKind, PrefixOperator},
        source::Span,
        type_checking::{BuiltinType, UnitExpressionId},
    };

    use super::super::{
        LoweredValue, UnitExpressionLowerer, builtin_type, cfg, lowering_error,
        parse_integer_literal, require_value, resolve_concrete_type,
    };
    use crate::ssa::{
        LoweringError, LoweringErrorKind,
        expression_core::checked::{self, CheckedCarry, CheckedLowering},
        integer::{bitwise_operator, checked_operator, comparison_operator},
        model::{
            BlockId, CheckedArithmeticOperator, EntityId, EntityType, Function, LoanId, Operation,
            Origin, ScalarConstant, SsaTypeId, ValueId,
        },
    };

    pub(in crate::ssa) struct CheckedRoles {
        baseline: BTreeMap<UnitSymbolId, LoweredValue>,
        baseline_borrows: BTreeMap<UnitSymbolId, LoanId>,
        carried: Vec<cfg::CarriedBinding>,
        carried_loans: Vec<cfg::CarriedAccess>,
    }

    impl UnitExpressionLowerer<'_> {
        pub(in crate::ssa::unit_lower) fn checked(
            &mut self,
            operator: CheckedArithmeticOperator,
            left: ValueId,
            right: ValueId,
            ty: SsaTypeId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            checked::lower_checked_arithmetic(self, operator, left, right, ty, span)
        }
    }

    impl CheckedLowering for UnitExpressionLowerer<'_> {
        type Roles = CheckedRoles;

        fn boolean_type(&self) -> Option<SsaTypeId> {
            self.type_ids.iter().find_map(|(frontend, ssa)| {
                (builtin_type(self.typed, *frontend) == Some(BuiltinType::Boolean)).then_some(*ssa)
            })
        }

        fn ir_cursor(&mut self) -> (&mut Function, &mut BlockId) {
            (&mut *self.function, &mut self.block)
        }

        fn capture_roles(&self, span: Span) -> Result<CheckedCarry<Self::Roles>, LoweringError> {
            let baseline = self.bindings.clone();
            let baseline_borrows = self.borrow_bindings.clone();
            let mut carried = if self
                .pending_operands
                .iter()
                .any(|entity| matches!(entity, EntityId::Place(_)))
            {
                // Receiver reservation/writeback 的 Copyable original 也必须与具名 binding 同步重绑。
                self.carried_bindings(&baseline, span)?
            } else {
                self.move_only_carried_bindings(&baseline, span)?
            };
            let mut carried_loans = self.carried_loans(&baseline_borrows, span)?;
            self.carry_pending_operands(&mut carried, &mut carried_loans, span)?;
            Ok(CheckedCarry {
                columns: cfg::checked_carry_columns(&carried, &carried_loans),
                roles: CheckedRoles {
                    baseline,
                    baseline_borrows,
                    carried,
                    carried_loans,
                },
            })
        }

        fn restore_roles(
            &mut self,
            roles: Self::Roles,
            _from: BlockId,
            success: BlockId,
            span: Span,
        ) -> Result<(), LoweringError> {
            self.bindings = self.rebind_carried_control(
                &roles.baseline,
                success,
                &roles.carried,
                &roles.carried_loans,
                span,
            )?;
            self.borrow_bindings = self.rebind_carried_loans(
                success,
                roles.carried.len(),
                &roles.carried_loans,
                span,
            )?;
            Ok(())
        }
    }

    impl UnitExpressionLowerer<'_> {
        pub(in crate::ssa::unit_lower) fn lower_prefix(
            &mut self,
            operator: PrefixOperator,
            operand: ExpressionId,
            expression: ExpressionId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let operand_type = self.expression_builtin_type(operand, span)?;
            let supported = match operator {
                PrefixOperator::Not => operand_type == Some(BuiltinType::Boolean),
                PrefixOperator::Plus | PrefixOperator::Minus => {
                    operand_type.is_some_and(is_integer_builtin)
                }
            };
            if !supported {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            if operator == PrefixOperator::Minus {
                let operand_node = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(operand)
                    .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
                if let Expression::Literal(LiteralKind::Integer(kind)) = operand_node.payload() {
                    let constant = parse_integer_literal(self.sources, *kind, operand_node.span())?
                        .checked_neg()
                        .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidLiteral, span))?;
                    let ty = self.expression_ssa_type(expression, span)?;
                    return self.append_scalar(
                        Operation::Constant(ScalarConstant::Integer(constant)),
                        ty,
                        span,
                    );
                }
            }

            let operand = match self.lower(operand)? {
                LoweredValue::Value(value) => value,
                LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                LoweredValue::Unit => {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            };
            match operator {
                PrefixOperator::Plus => Ok(LoweredValue::Value(operand)),
                PrefixOperator::Not => {
                    let ty = self.expression_ssa_type(expression, span)?;
                    self.append_scalar(Operation::BooleanNot { operand }, ty, span)
                }
                PrefixOperator::Minus => {
                    let ty = self.expression_ssa_type(expression, span)?;
                    let zero = self.append_scalar(
                        Operation::Constant(ScalarConstant::Integer(0)),
                        ty,
                        span,
                    )?;
                    let LoweredValue::Value(zero) = zero else {
                        return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                    };
                    self.checked(CheckedArithmeticOperator::Subtract, zero, operand, ty, span)
                }
            }
        }

        pub(in crate::ssa::unit_lower) fn lower_scalar_binary(
            &mut self,
            left: ExpressionId,
            operator: BinaryOperator,
            right: ExpressionId,
            expression: ExpressionId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            if matches!(
                operator,
                BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr
            ) {
                return self.lower_short_circuit(left, operator, right, expression, span);
            }
            if let Some(result) = self.lower_nullable_comparison(expression, span)? {
                return Ok(result);
            }
            let operand_type = self.expression_builtin_type(left, span)?;
            if operand_type == Some(BuiltinType::String) {
                return self.lower_string_binary(left, operator, right, expression, span);
            }
            let left_is_bottom = operand_type == Some(BuiltinType::Nothing);
            let supported = match operator {
                BinaryOperator::Add
                | BinaryOperator::Subtract
                | BinaryOperator::Multiply
                | BinaryOperator::Divide
                | BinaryOperator::Remainder
                | BinaryOperator::BitwiseAnd
                | BinaryOperator::BitwiseOr
                | BinaryOperator::BitwiseXor
                | BinaryOperator::Shl
                | BinaryOperator::Shr
                | BinaryOperator::Ushr
                | BinaryOperator::Less
                | BinaryOperator::LessEqual
                | BinaryOperator::Greater
                | BinaryOperator::GreaterEqual => operand_type.is_some_and(is_integer_builtin),
                BinaryOperator::Equal | BinaryOperator::NotEqual => {
                    left_is_bottom
                        || matches!(operand_type, Some(BuiltinType::Boolean | BuiltinType::Char))
                        || operand_type.is_some_and(is_integer_builtin)
                }
                _ => false,
            };
            if !supported {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            let left = match self.lower(left)? {
                LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                LoweredValue::Value(value) if !left_is_bottom => value,
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            };
            let right = match self.lower(right)? {
                LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                LoweredValue::Value(value) => value,
                LoweredValue::Unit => {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            };
            let ty = self.expression_ssa_type(expression, span)?;
            if let Some(operator) = checked_operator(operator) {
                return self.checked(operator, left, right, ty, span);
            }
            if let Some(operator) = bitwise_operator(operator) {
                return self.append_scalar(
                    Operation::IntegerBitwise {
                        operator,
                        left,
                        right,
                    },
                    ty,
                    span,
                );
            }
            let operator = comparison_operator(operator)
                .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
            self.append_scalar(
                Operation::Compare {
                    operator,
                    left,
                    right,
                },
                ty,
                span,
            )
        }

        fn lower_string_binary(
            &mut self,
            left: ExpressionId,
            operator: BinaryOperator,
            right: ExpressionId,
            expression: ExpressionId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            self.reject_live_if_proof(span)?;
            let Some(left) = self.lower_string_view(left, span)? else {
                return Ok(LoweredValue::Diverged);
            };
            // RHS 的控制流会重绑定 owner；保留槽位而非跨 edge 复用旧 ValueId。
            let pending_start = self.pending_operands.len();
            self.pending_operands.push(left);
            let right = self.lower_string_view(right, span)?;
            let left = self.pending_operands.get(pending_start).copied();
            self.pending_operands.truncate(pending_start);
            let Some(right) = right else {
                return Ok(LoweredValue::Diverged);
            };
            let left = left.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let ty = self.expression_ssa_type(expression, span)?;
            let operation = match operator {
                BinaryOperator::Add => Operation::StringConcat { left, right },
                BinaryOperator::Equal | BinaryOperator::NotEqual => {
                    Operation::StringEqual { left, right }
                }
                _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            };
            let result = self.append_scalar(operation, ty, span)?;
            self.emit_drops(UnitDropPoint::AfterBinaryOperands(UnitExpressionId::new(
                self.source_unit,
                expression,
            )))?;
            if operator != BinaryOperator::NotEqual {
                return Ok(result);
            }
            let LoweredValue::Value(result) = result else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            self.append_scalar(Operation::BooleanNot { operand: result }, ty, span)
        }

        fn lower_string_view(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<Option<crate::ssa::model::EntityId>, LoweringError> {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            // 常量 Name/Member 是逐 use temporary，不从声明的 variable binding 读取。
            if self
                .constant_owned
                .and_then(|owned| {
                    owned.materialization_at(UnitExpressionId::new(self.source_unit, expression))
                })
                .is_some()
            {
                return self.lower_string_temporary_view(expression, span);
            }
            match node.payload() {
                Expression::Group { expression } => self.lower_string_view(*expression, span),
                Expression::This => {
                    // A trusted extension's synthetic receiver is an ordinary entry loan.
                    // Nominal this retains the existing receiver path when no such binding exists.
                    match self
                        .references
                        .get(&super::super::span_key(node.span()))
                        .and_then(|symbol| self.borrow_bindings.get(symbol))
                        .copied()
                    {
                        Some(loan) => Ok(Some(EntityId::Loan(loan))),
                        None => self.lower_string_temporary_view(expression, span),
                    }
                }
                Expression::Name => {
                    let symbol = self
                        .references
                        .get(&super::super::span_key(node.span()))
                        .ok_or_else(|| {
                            lowering_error(LoweringErrorKind::MissingFact, node.span())
                        })?;
                    if let Some(LoweredValue::Value(value)) = self.bindings.get(symbol).copied() {
                        return Ok(Some(EntityId::Value(value)));
                    }
                    // Borrow 参数复用当前 active loan，CFG 已按 binding 重绑定其身份。
                    self.borrow_bindings
                        .get(symbol)
                        .copied()
                        .map(|loan| Some(EntityId::Loan(loan)))
                        .ok_or_else(|| {
                            lowering_error(LoweringErrorKind::UnsupportedNode, node.span())
                        })
                }
                _ => self.lower_string_temporary_view(expression, span),
            }
        }

        fn lower_string_temporary_view(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<Option<crate::ssa::model::EntityId>, LoweringError> {
            match self.lower(expression)? {
                LoweredValue::Value(value) => Ok(Some(crate::ssa::model::EntityId::Value(value))),
                LoweredValue::Diverged => Ok(None),
                LoweredValue::Unit => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            }
        }

        pub(in crate::ssa::unit_lower) fn expression_builtin_type(
            &self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<Option<BuiltinType>, LoweringError> {
            let ty = self
                .typed
                .expression_type(UnitExpressionId::new(self.source_unit, expression))
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let concrete =
                resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span)?;
            Ok(builtin_type(self.typed, concrete))
        }

        pub(in crate::ssa::unit_lower) fn materialize_unit_value(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<ValueId, LoweringError> {
            if self.expression_builtin_type(expression, span)? != Some(BuiltinType::Unit) {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let ty = self.expression_ssa_type(expression, span)?;
            match self.append_scalar(Operation::Constant(ScalarConstant::Unit), ty, span)? {
                LoweredValue::Value(value) => Ok(value),
                LoweredValue::Unit | LoweredValue::Diverged => {
                    Err(lowering_error(LoweringErrorKind::InvalidModel, span))
                }
            }
        }

        pub(in crate::ssa::unit_lower) fn append_scalar(
            &mut self,
            operation: Operation,
            ty: SsaTypeId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let (_, results) = self
                .function
                .append_instruction(
                    self.block,
                    operation,
                    vec![EntityType::Value(ty)],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            Ok(LoweredValue::Value(require_value(results[0], span)?))
        }
    }

    pub(in crate::ssa::unit_lower) const fn is_integer_builtin(builtin: BuiltinType) -> bool {
        matches!(
            builtin,
            BuiltinType::Byte
                | BuiltinType::Short
                | BuiltinType::Int
                | BuiltinType::Long
                | BuiltinType::UByte
                | BuiltinType::UShort
                | BuiltinType::UInt
                | BuiltinType::ULong
        )
    }
}
