//! String Borrow operand 的完成点与提前退出清理。
use super::*;

impl DropPlanner<'_, '_> {
    pub(super) fn is_string_expression(&self, expression: ExpressionId) -> bool {
        self.checker
            .typed
            .expression_type(expression)
            .and_then(|ty| self.checker.typed.types().get(ty))
            == Some(&TypeKind::Builtin(BuiltinType::String))
    }

    pub(super) fn string_binary(
        &mut self,
        left: ExpressionId,
        right: ExpressionId,
        binary: ExpressionId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let (continues, left_drop) = self.string_view_operand(left, state)?;
        if !continues {
            return Ok(false);
        }
        // The right operand can leave this expression before the operation executes.
        // Keep the completed left temporary in the existing control-transfer cleanup stack.
        if let Some(StringOperandDrop::Temporary(subject, origin)) = left_drop {
            state.nullable_temporaries.push(NullableTemporary {
                versions: Vec::new(),
                closures: Vec::new(),
                transfers_at_call: false,
                control: binary,
                subject,
                origin,
                loop_depth: self.loop_boundaries.len(),
                prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
            });
        }
        let (continues, right_drop) = self.string_view_operand(right, state)?;
        // Normal completion uses AfterBinaryOperands below. A terminated path has either
        // already cleaned the obligation at its transfer, or aborted without unwinding.
        state
            .nullable_temporaries
            .retain(|temporary| temporary.control != binary);
        if !continues {
            return Ok(false);
        }
        let point = DropPoint::AfterBinaryOperands(binary);
        for pending in [right_drop, left_drop].into_iter().flatten() {
            match pending {
                StringOperandDrop::Named(symbol) => self.drop_named_asap(point, symbol, state),
                StringOperandDrop::Temporary(expression, origin) => self.push_fact(DropFact::new(
                    point,
                    DropTarget::Temporary(expression),
                    origin,
                )),
            }
        }
        Ok(true)
    }

    pub(super) fn string_view_operand(
        &mut self,
        expression: ExpressionId,
        state: &mut ValueState,
    ) -> Result<(bool, Option<StringOperandDrop>), OwnershipCheckingError> {
        let node = self.checker.parsed.ast().expressions().get(expression)?;
        if self.checker.is_constant_use(expression) {
            return Ok((
                true,
                Some(StringOperandDrop::Temporary(expression, node.span())),
            ));
        }
        match node.payload() {
            Expression::Group { expression } => self.string_view_operand(*expression, state),
            Expression::Name => {
                let Some(symbol) = self.checker.reference_symbol(node.span()) else {
                    return Ok((true, None));
                };
                Ok((
                    true,
                    (!self.liveness.expression_after[expression.index()].contains(&symbol))
                        .then_some(StringOperandDrop::Named(symbol)),
                ))
            }
            _ => {
                if !self.expression(expression, ExpressionUse::Read, state)? {
                    return Ok((false, None));
                }
                if !self.is_move_only_temporary(expression) {
                    return Ok((true, None));
                }
                Ok((
                    true,
                    Some(StringOperandDrop::Temporary(expression, node.span())),
                ))
            }
        }
    }
}
