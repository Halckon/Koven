//! Assignment and compound-assignment lowering in `ssa::refine`.
use lang_frontend::{
    ast::ExpressionId, ownership_checking::UnitDropPoint, parser::AssignmentOperator,
    parser::Expression, source::Span,
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{CheckedArithmeticOperator, EntityType},
    refine::body::{RAbort, ROp, RTerm, Slot},
};

impl Builder<'_, '_> {
    pub(super) fn assignment(
        &mut self,
        target: ExpressionId,
        value: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let symbol = self.reference(self.span(target))?;
        let Some(slot) = self.required(value)? else {
            return Ok(Lowered::Diverged);
        };
        let declared = self
            .symbol_type(symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let slot = self.adapt(slot, value, declared)?;
        self.deliver(value);
        if let Some(loan) = self.inout_loan(symbol) {
            // The caller's place now holds the new value; nothing is rebound here.
            self.push(ROp::LoanMutate { loan, value: slot }, Vec::new(), span)?;
            return Ok(Lowered::Unit);
        }
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.end_views_of(symbol, span)?;
        self.sync_dead();
        self.dead.remove(&symbol);
        self.push(ROp::Alias { source: slot }, vec![binding], span)?;
        self.narrowed.remove(&symbol);
        Ok(Lowered::Unit)
    }

    pub(super) fn compound_assignment(
        &mut self,
        expression: ExpressionId,
        target: ExpressionId,
        operator: AssignmentOperator,
        value: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        if self.is_element_target(target) {
            return self.element_compound_assignment(expression, target, operator, value, span);
        }
        let symbol = self.reference(self.span(target))?;
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(right) = self.required(value)? else {
            return Ok(Lowered::Diverged);
        };
        let ty = self.ssa_type(self.expression_type(target)?, span)?;
        let boolean = self
            .inputs
            .typed
            .types()
            .builtin(lang_frontend::type_checking::BuiltinType::Boolean)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let boolean = self.ssa_type(boolean, span)?;
        let operator = match operator {
            AssignmentOperator::AddAssign => CheckedArithmeticOperator::Add,
            AssignmentOperator::SubtractAssign => CheckedArithmeticOperator::Subtract,
            AssignmentOperator::MultiplyAssign => CheckedArithmeticOperator::Multiply,
            AssignmentOperator::DivideAssign => CheckedArithmeticOperator::Divide,
            AssignmentOperator::RemainderAssign => CheckedArithmeticOperator::Remainder,
            AssignmentOperator::Assign => {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        let result = self.body.slot(EntityType::Value(ty));
        let failed = self.body.slot(EntityType::Value(boolean));
        self.push(
            ROp::CheckedArithmetic {
                operator,
                left: binding,
                right,
            },
            vec![result, failed],
            span,
        )?;
        let abort = self.body.block();
        let success = self.body.block();
        self.terminate(
            RTerm::Branch {
                condition: failed,
                when_true: abort,
                when_false: success,
            },
            span,
        )?;
        self.block = Some(abort);
        self.terminate(
            RTerm::Abort(RAbort::CheckedArithmetic {
                operator,
                divisor: right,
            }),
            span,
        )?;
        self.block = Some(success);
        self.end_views_of(symbol, span)?;
        self.sync_dead();
        self.dead.remove(&symbol);
        self.push(ROp::Alias { source: result }, vec![binding], span)?;
        self.narrowed.remove(&symbol);
        Ok(Lowered::Unit)
    }

    fn element_compound_assignment(
        &mut self,
        assignment: ExpressionId,
        target: ExpressionId,
        operator: AssignmentOperator,
        value: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let mut root_target = target;
        while let (Expression::Group { expression: inner }, _) = self.node(root_target)? {
            root_target = inner;
        }
        let descriptor = self
            .inputs
            .typed
            .element_place(self.unit(root_target))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let Some(index) = self.required(descriptor.index().expression())? else {
            let replaced =
                self.collect_drops(UnitDropPoint::AfterReplacement(self.unit(assignment)));
            self.finish_drops(&replaced);
            return Ok(Lowered::Diverged);
        };
        // 元素槽位定位与边界检查必须先于 RHS 求值，否则 RHS 副作用会在越界 abort 前生效。
        let initial_owner = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element_ty = self.c(descriptor.element_type());
        let ssa_element_ty = self.ssa_type(element_ty, span)?;
        let place = self.body.slot(EntityType::Place(ssa_element_ty));
        self.push(
            ROp::ContainerElementPlace {
                owner: initial_owner,
                index,
            },
            vec![place],
            span,
        )?;
        let current = self.body.slot(EntityType::Value(ssa_element_ty));
        self.push(ROp::ReadPlace { place }, vec![current], span)?;
        let Some(right) = self.required(value)? else {
            let replaced =
                self.collect_drops(UnitDropPoint::AfterReplacement(self.unit(assignment)));
            self.finish_drops(&replaced);
            return Ok(Lowered::Diverged);
        };
        self.deliver(value);
        let boolean = self
            .inputs
            .typed
            .types()
            .builtin(lang_frontend::type_checking::BuiltinType::Boolean)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let boolean = self.ssa_type(boolean, span)?;
        let operator = match operator {
            AssignmentOperator::AddAssign => CheckedArithmeticOperator::Add,
            AssignmentOperator::SubtractAssign => CheckedArithmeticOperator::Subtract,
            AssignmentOperator::MultiplyAssign => CheckedArithmeticOperator::Multiply,
            AssignmentOperator::DivideAssign => CheckedArithmeticOperator::Divide,
            AssignmentOperator::RemainderAssign => CheckedArithmeticOperator::Remainder,
            AssignmentOperator::Assign => {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        let result = self.body.slot(EntityType::Value(ssa_element_ty));
        let failed = self.body.slot(EntityType::Value(boolean));
        self.push(
            ROp::CheckedArithmetic {
                operator,
                left: current,
                right,
            },
            vec![result, failed],
            span,
        )?;
        let abort = self.body.block();
        let success = self.body.block();
        self.terminate(
            RTerm::Branch {
                condition: failed,
                when_true: abort,
                when_false: success,
            },
            span,
        )?;
        self.block = Some(abort);
        self.terminate(
            RTerm::Abort(RAbort::CheckedArithmetic {
                operator,
                divisor: right,
            }),
            span,
        )?;
        self.block = Some(success);
        // RHS 可能重新绑定容器变量，写回必须使用 RHS 之后的绑定。
        let write_owner = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.push(
            ROp::ContainerReplace {
                owner: write_owner,
                index,
                value: result,
            },
            Vec::new(),
            span,
        )?;
        let replaced = self.collect_drops(UnitDropPoint::AfterReplacement(self.unit(assignment)));
        self.finish_drops(&replaced);
        Ok(Lowered::Unit)
    }

    pub(super) fn snapshot_binding_if_name(
        &mut self,
        expression: ExpressionId,
        slot: Slot,
    ) -> Result<Slot, LoweringError> {
        let mut target = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(target)? {
            target = inner;
        }
        if let (Expression::Name, span) = self.node(target)?
            && let Ok(symbol) = self.reference(span)
            && self
                .symbol_type(symbol)
                .is_some_and(|ty| !self.move_only(ty))
            && self.bindings.get(&symbol).copied() == Some(slot)
        {
            let snapshot = self.body.slot(self.body.slots[slot.0]);
            self.push(ROp::Alias { source: slot }, vec![snapshot], span)?;
            return Ok(snapshot);
        }
        Ok(slot)
    }

    /// A Borrow argument of type `T` for a `T?` parameter: the temporary is wrapped and
    /// its drop obligation moves to the wrapper.
    pub(super) fn adapt_borrowed(
        &mut self,
        value: Slot,
        expression: ExpressionId,
        parameter_type: lang_frontend::type_checking::UnitTypeId,
    ) -> Result<Slot, LoweringError> {
        let id = self.unit(expression);
        let temporary = self.temporaries.contains_key(&id);
        let adapted = self.adapt(value, expression, self.c(parameter_type))?;
        if adapted != value && temporary {
            self.temporaries.insert(id, adapted);
        }
        Ok(adapted)
    }

    pub(super) fn return_expression(
        &mut self,
        expression: ExpressionId,
        value: Option<ExpressionId>,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        if self.inputs.borrow_return.is_some() {
            let value = value.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            return self.lower_borrow_return(expression, value, span);
        }
        if self.inputs.carrier_return.is_some() {
            let value = value.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            return self.lower_carrier_return(expression, value, span);
        }
        let result = match value {
            Some(value) => {
                let saved_expected = self.expected_nullable;
                self.expected_nullable = if matches!(
                    self.inputs.typed.types().get(self.inputs.return_type),
                    Some(lang_frontend::type_checking::UnitTypeKind::Nullable(_))
                ) {
                    self.ssa_type(self.inputs.return_type, span).ok()
                } else {
                    None
                };
                let slot = self.required(value)?;
                self.expected_nullable = saved_expected;
                let Some(slot) = slot else {
                    return Ok(Lowered::Diverged);
                };
                let slot = self.adapt(slot, value, self.inputs.return_type)?;
                self.deliver(value);
                Some(slot)
            }
            None => None,
        };
        for loan in std::mem::take(&mut self.pending_call_loans)
            .into_iter()
            .rev()
        {
            self.end_short_call_loan(loan, span)?;
        }
        self.end_all_views(span)?;
        self.drops(UnitDropPoint::ControlTransfer(self.unit(expression)))?;
        let unit_return = self.is_unit(self.inputs.return_type);
        self.finish_return(result, unit_return, span)?;
        Ok(Lowered::Diverged)
    }
}
