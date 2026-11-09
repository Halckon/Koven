//! String Borrow operand 的完成点与提前退出清理。
use super::*;

impl DropPlanner<'_, '_> {
    /// Append only after the receiver's live traversal has completed normally.
    /// This observation does not participate in pending loans or owner cleanup.
    pub(super) fn observe_string_clone_return(&mut self, expression: ExpressionId) {
        let call = self.checker.unit_expression(expression);
        let Some(operation) = self
            .checker
            .typed
            .string_operation(call)
            .filter(|operation| {
                operation.kind() == crate::type_checking::StringOperationKind::Clone
            })
        else {
            return;
        };
        self.string_clone_loan_ends.extend(
            self.checker
                .loans
                .iter()
                .filter(|loan| {
                    loan.call() == call
                        && loan.argument() == operation.receiver()
                        && loan.kind() == crate::ownership_checking::LoanKind::Shared
                })
                .cloned()
                .map(crate::ownership_checking::UnitStringCloneLoanEndFact::new),
        );
    }

    pub(super) fn is_string_expression(&self, expression: ExpressionId) -> bool {
        self.checker
            .typed
            .expression_type(self.checker.unit_expression(expression))
            .and_then(|ty| self.checker.typed.types().get(ty))
            == Some(&UnitTypeKind::Builtin(BuiltinType::String))
    }

    pub(super) fn uses_string_binary_operands(
        &self,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
    ) -> bool {
        if !matches!(
            operator,
            BinaryOperator::Add | BinaryOperator::Equal | BinaryOperator::NotEqual
        ) || !self.is_string_expression(left)
        {
            return false;
        }
        let right = self
            .checker
            .typed
            .expression_type(self.checker.unit_expression(right))
            .and_then(|ty| self.checker.typed.types().get(ty));
        right == Some(&UnitTypeKind::Builtin(BuiltinType::String))
            // A bottom RHS never reaches the operation, but its return still owns
            // the cleanup of the String temporary from the already evaluated left.
            || (matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual)
                && right == Some(&UnitTypeKind::Builtin(BuiltinType::Nothing)))
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
        // The completed left owner must survive until the right operand finishes or exits.
        if let Some(StringOperandDrop::Temporary(expression, origin)) = left_drop {
            self.register_pending_temporary(binary, expression, origin, false, state)?;
        }
        if let Some(StringOperandDrop::Named(symbol)) = left_drop {
            state.pending_borrows.push((binary, symbol));
        }
        let (continues, right_drop) = self.string_view_operand(right, state)?;
        // Transfers already clean pending owners; Abort never unwinds. Normal cleanup is below.
        state
            .pending_temporaries
            .retain(|pending| pending.control != binary);
        state
            .pending_borrows
            .retain(|(control, _)| *control != binary);
        if !continues {
            return Ok(false);
        }
        let point = PlannerDropPoint::AfterBinaryOperands(binary);
        for pending in [right_drop, left_drop].into_iter().flatten() {
            match pending {
                StringOperandDrop::Named(symbol) => {
                    if !self.liveness.expression_after[binary.index()].contains(&symbol) {
                        self.drop_named_asap(point, symbol, state);
                    }
                }
                StringOperandDrop::Temporary(expression, origin) => self.push_fact(
                    PlannerDropFact::new(point, PlannerDropTarget::Temporary(expression), origin),
                ),
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
        match node.payload() {
            Expression::Group { expression } => self.string_view_operand(*expression, state),
            Expression::Name if !self.checker.is_constant_use(expression) => {
                let Some(symbol) = self.checker.reference_symbol(node.span()) else {
                    return Ok((true, None));
                };
                Ok((true, Some(StringOperandDrop::Named(symbol))))
            }
            _ => {
                if !self.expression(expression, DropExpressionUse::Read, state)? {
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
