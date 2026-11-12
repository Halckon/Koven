//! Integer intrinsics (`inv`, checked conversions), prefix `+`/`-`, `IntRange`/`LongRange`
//! value operations, `String.clone()`, and trusted text primitives in the refined CFG (SPEC-0371).
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    parser::{Expression, LiteralKind, PrefixOperator},
    source::Span,
    type_checking::{
        BuiltinType, IntegerOperationKind, IntegerRangeOperationKind, ParameterMode,
        StringOperationKind, TextPrimitiveKind,
    },
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{CheckedArithmeticOperator, EntityType, LoanKind, ScalarConstant},
    refine::body::{RAbort, ROp, RTerm, Slot},
};

impl Builder<'_, '_> {
    pub(super) fn prefix(
        &mut self,
        operator: PrefixOperator,
        operand: ExpressionId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        if operator == PrefixOperator::Not {
            let Some(operand) = self.required(operand)? else {
                return Ok(Lowered::Diverged);
            };
            return Ok(Lowered::Value(
                self.typed_value(ROp::BooleanNot { operand }, expression)?,
            ));
        }
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        if operator == PrefixOperator::Minus {
            let (payload, operand_span) = self.node(operand)?;
            if let Expression::Literal(LiteralKind::Integer(kind)) = payload {
                let text = self
                    .inputs
                    .sources
                    .slice(operand_span)
                    .map_err(|_| error(LoweringErrorKind::MismatchedSource, span))?;
                let magnitude = lang_frontend::type_checking::integer_literal_magnitude(text, kind)
                    .ok_or_else(|| error(LoweringErrorKind::InvalidLiteral, span))?;
                let value = if magnitude == (1u128 << 127) {
                    i128::MIN
                } else {
                    i128::try_from(magnitude)
                        .ok()
                        .and_then(i128::checked_neg)
                        .ok_or_else(|| error(LoweringErrorKind::InvalidLiteral, span))?
                };
                return Ok(Lowered::Value(self.value(
                    ROp::Constant(ScalarConstant::Integer(value)),
                    EntityType::Value(ty),
                    span,
                )?));
            }
        }
        let Some(operand) = self.required(operand)? else {
            return Ok(Lowered::Diverged);
        };
        match operator {
            PrefixOperator::Plus => Ok(Lowered::Value(operand)),
            PrefixOperator::Minus => {
                let zero = self.value(
                    ROp::Constant(ScalarConstant::Integer(0)),
                    EntityType::Value(ty),
                    span,
                )?;
                let boolean = self
                    .inputs
                    .typed
                    .types()
                    .builtin(BuiltinType::Boolean)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let boolean = self.ssa_type(boolean, span)?;
                let result = self.body.slot(EntityType::Value(ty));
                let failed = self.body.slot(EntityType::Value(boolean));
                self.push(
                    ROp::CheckedArithmetic {
                        operator: CheckedArithmeticOperator::Subtract,
                        left: zero,
                        right: operand,
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
                        operator: CheckedArithmeticOperator::Subtract,
                        divisor: operand,
                    }),
                    span,
                )?;
                self.block = Some(success);
                Ok(Lowered::Value(result))
            }
            PrefixOperator::Not => unreachable!("handled above"),
        }
    }

    pub(super) fn integer_operation(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .integer_operation(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(operand) = self.required(descriptor.receiver().expression())? else {
            return Ok(Lowered::Diverged);
        };
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        match descriptor.kind() {
            IntegerOperationKind::Invert => Ok(Lowered::Value(self.value(
                ROp::IntegerNot { operand },
                EntityType::Value(ty),
                span,
            )?)),
            IntegerOperationKind::CheckedConvert => {
                let boolean = self
                    .inputs
                    .typed
                    .types()
                    .builtin(BuiltinType::Boolean)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let boolean = self.ssa_type(boolean, span)?;
                let result = self.body.slot(EntityType::Value(ty));
                let failed = self.body.slot(EntityType::Value(boolean));
                self.push(
                    ROp::CheckedIntegerConvert { operand },
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
                self.terminate(RTerm::Abort(RAbort::CheckedConversion { target: ty }), span)?;
                self.block = Some(success);
                Ok(Lowered::Value(result))
            }
        }
    }

    pub(super) fn integer_range_operation(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .integer_range_operation(id)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(left) = self.required(descriptor.left().expression())? else {
            return Ok(Lowered::Diverged);
        };
        let Some(right) = self.required(descriptor.right().expression())? else {
            return Ok(Lowered::Diverged);
        };
        let (op, negate) = match descriptor.kind() {
            IntegerRangeOperationKind::Inclusive | IntegerRangeOperationKind::Exclusive => (
                ROp::IntegerRangeConstruct {
                    first: left,
                    bound: right,
                    inclusive: descriptor.kind() == IntegerRangeOperationKind::Inclusive,
                },
                false,
            ),
            IntegerRangeOperationKind::Contains | IntegerRangeOperationKind::NotContains => (
                ROp::IntegerRangeContains {
                    value: left,
                    range: right,
                },
                descriptor.kind() == IntegerRangeOperationKind::NotContains,
            ),
            IntegerRangeOperationKind::Equal | IntegerRangeOperationKind::NotEqual => (
                ROp::IntegerRangeEqual { left, right },
                descriptor.kind() == IntegerRangeOperationKind::NotEqual,
            ),
        };
        let mut result = self.typed_value(op, expression)?;
        if negate {
            result = self.typed_value(ROp::BooleanNot { operand: result }, expression)?;
        }
        Ok(Lowered::Value(result))
    }

    pub(super) fn string_operation(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .string_operation(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.kind() != StringOperationKind::Clone
            || descriptor.receiver_mode() != ParameterMode::Borrow
            || descriptor.result_mode() != ParameterMode::Value
            || descriptor.receiver().source_unit() != self.inputs.source_unit
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let receiver = descriptor.receiver().expression();
        let receiver_ty = self.c(descriptor.receiver_type());
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let mut ungrouped = receiver;
        while let (Expression::Group { expression: inner }, _) = self.node(ungrouped)? {
            ungrouped = inner;
        }
        let (source, created_loans, view_reads) =
            if let Some(loan) = self.borrowed_string_loan(receiver)? {
                (loan, Vec::new(), vec![receiver])
            } else if let Some(view) = self.narrowed_view(receiver)? {
                (view, Vec::new(), vec![receiver])
            } else if let Some((loan, created, read)) =
                self.borrowed_rc_payload_loan(receiver, receiver_ty, span)?
            {
                (loan, created, read.into_iter().collect())
            } else if let Some((loan, created, read)) =
                self.borrowed_container_element_loan(receiver, receiver_ty, span)?
            {
                (loan, created, read.into_iter().collect())
            } else if self
                .inputs
                .typed
                .aggregate_projection(self.unit(ungrouped))
                .is_some()
            {
                let (loan, created, mut drops) = self.field_projection_loan(receiver, span)?;
                drops.push(receiver);
                (loan, created, drops)
            } else {
                let Some(owner) = self.required(receiver)? else {
                    return Ok(Lowered::Diverged);
                };
                let receiver_span = self.span(receiver);
                let place = self.value(
                    ROp::RootPlace { owner },
                    EntityType::Place(ty),
                    receiver_span,
                )?;
                let loan = self.value(
                    ROp::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: ty,
                    },
                    receiver_span,
                )?;
                (loan, vec![loan], Vec::new())
            };
        let result = self.value(ROp::StringClone { source }, EntityType::Value(ty), span)?;
        for loan in created_loans.into_iter().rev() {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        self.drops(UnitDropPoint::CallReturn(id))?;
        for read in view_reads {
            self.drops(UnitDropPoint::AfterExpression(self.unit(read)))?;
        }
        Ok(Lowered::Value(result))
    }

    pub(super) fn text_primitive_call(
        &mut self,
        expression: ExpressionId,
        kind: TextPrimitiveKind,
        lowered: &[Slot],
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let op = match (kind, lowered) {
            (TextPrimitiveKind::CopyBytes, [source]) => ROp::StringCopyBytes { source: *source },
            (TextPrimitiveKind::CopySlice, [source, start, end]) => {
                let start = self.text_index_slot(*start, span)?;
                let end = self.text_index_slot(*end, span)?;
                ROp::StringCopySlice {
                    source: *source,
                    start,
                    end,
                }
            }
            (TextPrimitiveKind::Utf8ErrorOffset, [source]) => {
                ROp::Utf8ErrorOffset { source: *source }
            }
            (TextPrimitiveKind::ByteLength, [source]) => ROp::StringByteLength { source: *source },
            (TextPrimitiveKind::ByteAt, [source, index]) => {
                let index = self.text_index_slot(*index, span)?;
                ROp::StringByteAt {
                    source: *source,
                    index,
                }
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        Ok(Lowered::Value(self.typed_value(op, expression)?))
    }

    fn text_index_slot(&mut self, loan: Slot, span: Span) -> Result<Slot, LoweringError> {
        let EntityType::Loan { target, .. } = self.body.slots[loan.0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        self.value(ROp::ReadLoan { loan }, EntityType::Value(target), span)
    }

    pub(super) fn numeric_for(
        &mut self,
        statement: lang_frontend::ast::StatementId,
        source: ExpressionId,
        loop_body: lang_frontend::ast::StatementId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        self.end_reassigned_loop_views(span)?;
        let statement_id =
            lang_frontend::type_checking::UnitStatementId::new(self.inputs.source_unit, statement);
        let plan = self
            .inputs
            .owned
            .iteration(statement_id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let descriptor = plan.descriptor();
        let endpoint = self.ssa_type(self.c(descriptor.element_type()), span)?;
        let boolean = self
            .inputs
            .typed
            .types()
            .builtin(BuiltinType::Boolean)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let boolean = self.ssa_type(boolean, span)?;
        let Some(range) = self.required(source)? else {
            return Ok(Lowered::Diverged);
        };
        let first = self.value(
            ROp::AggregateProject {
                aggregate: range,
                field: 0,
            },
            EntityType::Value(endpoint),
            span,
        )?;
        let last = self.value(
            ROp::AggregateProject {
                aggregate: range,
                field: 1,
            },
            EntityType::Value(endpoint),
            span,
        )?;
        let empty = self.value(
            ROp::AggregateProject {
                aggregate: range,
                field: 2,
            },
            EntityType::Value(boolean),
            span,
        )?;
        let current = self.value(
            ROp::Alias { source: first },
            EntityType::Value(endpoint),
            span,
        )?;
        let header = self.body.block();
        let continuation = self.body.block();
        let increment = self.body.block();
        let exhausted = self.body.block();
        let exit = self.body.block();
        self.terminate(
            RTerm::Branch {
                condition: empty,
                when_true: exhausted,
                when_false: header,
            },
            span,
        )?;
        let mark = self.enter(header);
        if let lang_frontend::type_checking::UnitSequentialIterationBinding::Name(symbol) =
            descriptor.binding()
        {
            let item = self.value(
                ROp::Alias { source: current },
                EntityType::Value(endpoint),
                span,
            )?;
            self.bindings.insert(*symbol, item);
        }
        self.loops.push(super::LoopFrame {
            statement: statement_id,
            continue_target: continuation,
            exit,
            views: mark,
            loans: self.pending_call_loans.len(),
            broken: false,
            seq_for: None,
            map_for: None,
        });
        let reached = self.statement(loop_body)? != Lowered::Diverged;
        self.loops.pop();
        self.leave(mark, span)?;
        if reached {
            self.goto(continuation, span)?;
        }
        let mark = self.enter(continuation);
        self.leave_unclaimed(mark, span)?;
        let terminal = self.value(
            ROp::Compare {
                operator: crate::ssa::model::ComparisonOperator::Equal,
                left: current,
                right: last,
            },
            EntityType::Value(boolean),
            span,
        )?;
        self.terminate(
            RTerm::Branch {
                condition: terminal,
                when_true: exhausted,
                when_false: increment,
            },
            span,
        )?;
        let mark = self.enter(increment);
        self.leave_unclaimed(mark, span)?;
        let one = self.value(
            ROp::Constant(ScalarConstant::Integer(1)),
            EntityType::Value(endpoint),
            span,
        )?;
        let next = self.value(
            ROp::Binary {
                operator: crate::ssa::model::BinaryOperator::Add,
                left: current,
                right: one,
            },
            EntityType::Value(endpoint),
            span,
        )?;
        self.push(ROp::Alias { source: next }, vec![current], span)?;
        self.goto(header, span)?;

        let facts = self.collect_drops(UnitDropPoint::LoopExit(statement_id));
        let mark = self.enter(exhausted);
        self.leave_unclaimed(mark, span)?;
        self.emit_drops(&facts)?;
        self.goto(exit, span)?;
        self.finish_drops(&facts);
        let mark = self.enter(exit);
        self.leave_unclaimed(mark, span)?;
        Ok(Lowered::Unit)
    }

    pub(super) fn constant_materialization(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<Lowered>, LoweringError> {
        let id = self.unit(expression);
        let Some(facts) = self.inputs.typed.constants() else {
            return Ok(None);
        };
        let Ok(index) = facts
            .uses()
            .binary_search_by_key(&id, |use_| use_.expression())
        else {
            return Ok(None);
        };
        let descriptor = &facts.uses()[index];
        let plan = self
            .inputs
            .constant_owned
            .and_then(|owned| owned.materialization_at(id))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if plan.descriptor() != descriptor {
            return Err(error(LoweringErrorKind::MismatchedAnalysis, span));
        }
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let op = match (plan.kind(), descriptor.value()) {
            (
                lang_frontend::ownership_checking::ConstantMaterializationKind::InlineCopy,
                lang_frontend::type_checking::ConstValue::Boolean(value),
            ) => ROp::Constant(ScalarConstant::Boolean(*value)),
            (
                lang_frontend::ownership_checking::ConstantMaterializationKind::InlineCopy,
                lang_frontend::type_checking::ConstValue::Integer { value, .. },
            ) => ROp::Constant(ScalarConstant::Integer(*value)),
            (
                lang_frontend::ownership_checking::ConstantMaterializationKind::InlineCopy,
                lang_frontend::type_checking::ConstValue::Char(value),
            ) => ROp::Constant(ScalarConstant::Char(u32::from(*value))),
            (
                lang_frontend::ownership_checking::ConstantMaterializationKind::StringTemporary,
                lang_frontend::type_checking::ConstValue::String(bytes),
            ) => ROp::StringLiteral {
                string: ty,
                bytes: bytes.to_vec(),
            },
            _ => return Err(error(LoweringErrorKind::MismatchedAnalysis, span)),
        };
        self.value(op, EntityType::Value(ty), span)
            .map(Lowered::Value)
            .map(Some)
    }
}

pub(in crate::ssa::refine::build) mod replacement {
    use super::super::ROp;
    #[cfg(test)]
    use super::super::owner_consumer_test_support::{OwnerFault, take_fault};
    use super::owner_consumer::DropReceipt;
    use super::{BlockRef, Builder, RTerm, RefineInputs, Slot};
    use crate::ssa::lowering_support::{LoweringError, LoweringErrorKind, error};
    use lang_frontend::{
        ast::ExpressionId, name_resolution::UnitSymbolId, source::Span,
        ownership_checking::{
            UnitDropPoint, UnitDropTarget, UnitOwnerOrigin,
            UnitOwnerReplaceBreakReturnActionV1 as ReturnAction,
            UnitOwnerReplaceBreakReturnConsumerV1,
        },
        type_checking::{OwnershipPrimitiveKind, UnitExpressionId},
    };

    pub(in crate::ssa::refine::build) struct ReturnReplacementPlan {
        pub(in crate::ssa::refine::build) call: UnitExpressionId,
        pub(in crate::ssa::refine::build) argument: UnitExpressionId,
        pub(in crate::ssa::refine::build) produced: Option<Slot>,
        pub(in crate::ssa::refine::build) old: Option<Slot>,
        pub(in crate::ssa::refine::build) drop_index: usize,
    }
    impl ReturnReplacementPlan {
        pub(in crate::ssa::refine::build) fn new(
            inputs: &RefineInputs<'_>,
            consumer: &UnitOwnerReplaceBreakReturnConsumerV1,
        ) -> Result<(Self, UnitSymbolId), LoweringError> {
            let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
            let [
                ReturnAction::Create(argument, owner),
                ReturnAction::Pending(call, pending_arg, pending),
                ReturnAction::Replace(commit, root, installed),
                ReturnAction::Drop(point, dropped_root, dropped),
                ReturnAction::Result(result),
                ReturnAction::Return { expression, payload },
            ] = consumer.actions()
            else {
                return Err(fail());
            };
            if argument != pending_arg || owner != pending || owner != installed || owner != dropped
                || call != commit || call != result || call != payload || root != dropped_root
                || *call != consumer.payload() || *expression != consumer.return_expression()
                || *point != UnitDropPoint::CallReturn(*call)
            {
                return Err(fail());
            }
            let catalog = consumer.replacement_producers().ok_or_else(fail)?;
            if catalog.definitions().len() != 1
                || catalog.definition(owner.clone()).map(|d| d.origin()) != Some(UnitOwnerOrigin::String(*argument))
            {
                return Err(fail());
            }
            let typed = inputs.typed.ownership_primitive(*call).ok_or_else(fail)?;
            let owned = inputs.owned.ownership_primitive(*call).ok_or_else(fail)?;
            if typed.kind() != OwnershipPrimitiveKind::Replace || typed.operands().len() != 2
                || typed.operands()[1] != *argument || *owned.descriptor() != typed
                || owned.places().len() != 1 || !owned.places()[0].is_root() || owned.places()[0].root() != *root
                || inputs.typed.types().get(typed.value_type())
                    != Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                        lang_frontend::type_checking::BuiltinType::String,
                    ))
            {
                return Err(fail());
            }
            let drops = inputs.owned.drops().iter().enumerate()
                .filter(|(_, d)| d.target() == UnitDropTarget::Named(*root))
                .collect::<Vec<_>>();
            let [(index, drop)] = drops.as_slice() else { return Err(fail()); };
            if drop.point() != *point { return Err(fail()); }
            Ok((Self { call: *call, argument: *argument, produced: None, old: None, drop_index: *index }, *root))
        }
    }
    impl Builder<'_, '_> {
        pub(in crate::ssa::refine::build) fn authorize_return_replace(
            &mut self,
            call: UnitExpressionId,
            root: Slot,
            replacement: Slot,
            span: Span,
        ) -> Result<bool, LoweringError> {
            let Some(plan) = &self.owner_plan else {
                return Ok(false);
            };
            let Some(frame) = plan.return_replacement.as_ref().filter(|r| r.call == call) else {
                return Ok(false);
            };
            let fail = || error(LoweringErrorKind::InvalidModel, span);
            let exit = plan.exit.ok_or_else(fail)?;
            let [Some(condition), Some(body)] = plan.incoming else {
                return Err(fail());
            };
            if plan.stage != 0
                || !plan.replaced
                || self.block != Some(exit)
                || plan.root_slot != Some(root)
                || self.bindings.get(&plan.root) != Some(&root)
                || self.dead.contains(&plan.root)
                || frame.produced != Some(replacement)
                || frame.old.is_some()
                || condition == body
                || plan.false_block != Some(condition)
            {
                return Err(fail());
            }
            let incoming = self
                .body
                .blocks
                .iter()
                .enumerate()
                .filter_map(|(index, b)| {
                    b.terminator
                        .as_ref()
                        .filter(|(term, _)| term.successors().contains(&exit))
                        .map(|_| BlockRef(index))
                })
                .collect::<Vec<_>>();
            if incoming.len() != 2
                || !incoming.contains(&condition)
                || !incoming.contains(&body)
                || [condition, body].iter().any(|edge| !matches!(self.body.blocks[edge.0].terminator, Some((RTerm::Goto(target), _)) if target == exit))
            {
                return Err(fail());
            }
            self.owner_plan.as_mut().ok_or_else(fail)?.stage = 7;
            Ok(true)
        }
        pub(in crate::ssa::refine::build) fn return_replace_emitted(
            &mut self,
            call: UnitExpressionId,
            root: Slot,
            replacement: Slot,
            old: Slot,
            span: Span,
        ) -> Result<(), LoweringError> {
            let Some(plan) = &mut self.owner_plan else {
                return Ok(());
            };
            let Some(frame) = plan.return_replacement.as_mut().filter(|r| r.call == call) else {
                return Ok(());
            };
            #[cfg(test)]
            if take_fault(OwnerFault::MissingReturnReplaceReceipt) {
                return Ok(());
            }
            #[cfg(test)]
            let old = if take_fault(OwnerFault::ReturnReplaceOldIsRoot) {
                root
            } else {
                old
            };
            if plan.stage != 7
                || plan.root_slot != Some(root)
                || frame.produced != Some(replacement)
                || old == root
                || old == replacement
                || frame.old.replace(old).is_some()
            {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            plan.stage = 8;
            #[cfg(test)]
            super::super::owner_consumer_test_support::record_action("replace_result", old.0);
            #[cfg(test)]
            if take_fault(OwnerFault::DuplicateReturnReplaceReceipt) {
                return self.return_replace_emitted(call, root, replacement, old, span);
            }
            Ok(())
        }
        pub(in crate::ssa::refine::build) fn authorize_return_replace_drop(
            &self,
            index: usize,
            target: UnitDropTarget,
            slot: Slot,
            span: Span,
        ) -> Result<Option<DropReceipt>, LoweringError> {
            let Some(plan) = &self.owner_plan else {
                return Ok(None);
            };
            let Some(frame) = &plan.return_replacement else {
                return Ok(None);
            };
            // Only the suffix's real cleanup may consume this CI-only mutation.
            #[cfg(test)]
            let slot = if index == frame.drop_index
                && target == UnitDropTarget::Named(plan.root)
                && take_fault(OwnerFault::ReturnReplaceDropIsOld)
            {
                frame
                    .old
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
            } else {
                slot
            };
            if frame.old == Some(slot) {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            if index != frame.drop_index && target != UnitDropTarget::Named(plan.root) {
                return Ok(None);
            }
            let fail = || error(LoweringErrorKind::InvalidModel, span);
            if plan.stage != 8
                || self.block != plan.exit
                || index != frame.drop_index
                || target != UnitDropTarget::Named(plan.root)
                || plan.root_slot != Some(slot)
                || !self.dead.contains(&plan.root)
                || self.consumed_drops.contains(&index)
            {
                return Err(fail());
            }
            Ok(Some(DropReceipt {
                selected: frame.old.ok_or_else(fail)?,
                bound: slot,
                index,
                replacement: true,
            }))
        }
        pub(in crate::ssa::refine::build) fn return_replace_drop_emitted(
            &mut self,
            receipt: DropReceipt,
            span: Span,
        ) -> Result<(), LoweringError> {
            let plan = self
                .owner_plan
                .as_mut()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let frame = plan
                .return_replacement
                .as_ref()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            #[cfg(test)]
            if take_fault(OwnerFault::MissingReturnReplaceDropReceipt) {
                return Ok(());
            }
            if plan.stage != 8
                || frame.old != Some(receipt.selected)
                || frame.drop_index != receipt.index
                || plan.root_slot != Some(receipt.bound)
            {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            plan.stage = 9;
            #[cfg(test)]
            super::super::owner_consumer_test_support::record_action(
                "replace_installed_drop",
                receipt.bound.0,
            );
            #[cfg(test)]
            if take_fault(OwnerFault::DuplicateReturnReplaceDropReceipt) {
                return self.return_replace_drop_emitted(receipt, span);
            }
            Ok(())
        }
    }

    impl Builder<'_, '_> {
        pub(in crate::ssa::refine::build) fn owner_unit_call(
            &mut self,
            call: UnitExpressionId,
            op: ROp,
            span: Span,
        ) -> Result<(), LoweringError> {
            #[cfg(test)]
            let op = {
                use super::super::owner_consumer_test_support::{OwnerFault, take_fault};
                let mut op = op;
                if self
                    .owner_plan
                    .as_ref()
                    .is_some_and(|p| p.call.is_some_and(|c| c.0 == call))
                {
                    if take_fault(OwnerFault::MissingOwnCall) {
                        return Ok(());
                    }
                    if take_fault(OwnerFault::WrongOwnCallSlot)
                        && let ROp::DirectCall { arguments, .. } = &mut op
                    {
                        let index = self
                            .owner_plan
                            .as_ref()
                            .and_then(|p| p.call_layout)
                            .map_or(0, |p| p.0);
                        arguments[index] = Slot(usize::MAX);
                    }
                }
                op
            };
            let receipt = if let Some(plan) = self
                .owner_plan
                .as_ref()
                .filter(|p| p.call.is_some_and(|c| c.0 == call))
            {
                let ROp::DirectCall {
                    callee,
                    receiver: None,
                    arguments,
                } = &op
                else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                let selected = plan
                    .root_slot
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let (parameter_index, arity) = plan
                    .call_layout
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if plan.call != Some((call, *callee))
                    || plan.stage != 1
                    || arguments.len() != arity
                    || arguments.get(parameter_index) != Some(&selected)
                    || arguments.iter().filter(|slot| **slot == selected).count() != 1
                    || !self.dead.contains(&plan.root)
                {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                }
                Some(selected)
            } else {
                None
            };
            #[cfg(test)]
            let actual = match &op {
                ROp::DirectCall {
                    callee, arguments, ..
                } => Some((*callee, arguments.clone())),
                _ => None,
            };
            #[cfg(test)]
            let replay = op.clone();
            self.push(op, Vec::new(), span)?;
            if receipt.is_some() {
                let plan = self
                    .owner_plan
                    .as_mut()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                plan.stage = 3;
                #[cfg(test)]
                if super::super::owner_consumer_test_support::take_fault(
                    super::super::owner_consumer_test_support::OwnerFault::DuplicateOwnCall,
                ) {
                    return self.owner_unit_call(call, replay, span);
                }
            }
            #[cfg(test)]
            if let Some((callee, arguments)) = actual {
                super::super::super::record_owner_call(
                    call,
                    callee,
                    &arguments,
                    receipt.map(|s| s.0),
                );
            }
            Ok(())
        }
    }

    impl Builder<'_, '_> {
        pub(in crate::ssa::refine::build) fn owner_return(
            &mut self,
            expression: ExpressionId,
            result: Option<Slot>,
            unit_return: bool,
            span: Span,
        ) -> Result<(), LoweringError> {
            let identity = self.unit(expression);
            #[cfg(test)]
            let result = {
                use super::super::owner_consumer_test_support::{OwnerFault, take_fault};
                if self
                    .owner_plan
                    .as_ref()
                    .is_some_and(|p| p.return_expression == Some(identity))
                {
                    if take_fault(OwnerFault::MissingOwnerReturn) {
                        return Ok(());
                    }
                    if take_fault(OwnerFault::WrongReturnPayload) {
                        Some(Slot(usize::MAX))
                    } else {
                        result
                    }
                } else {
                    result
                }
            };
            let receipt = if let Some(plan) = self
                .owner_plan
                .as_ref()
                .filter(|p| p.return_expression == Some(identity))
            {
                let (selected, required_stage) =
                    if let Some(replacement) = &plan.return_replacement {
                        if !self.consumed_drops.contains(&replacement.drop_index) {
                            return Err(error(LoweringErrorKind::InvalidModel, span));
                        }
                        (replacement.old, 9)
                    } else if let Some(binding) = &plan.return_binding {
                        if !self.dead.contains(&binding.symbol) {
                            return Err(error(LoweringErrorKind::InvalidModel, span));
                        }
                        (binding.slot, 4)
                    } else {
                        (plan.root_slot, 1)
                    };
                let selected =
                    selected.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if unit_return
                    || plan.stage != required_stage
                    || result != Some(selected)
                    || !self.dead.contains(&plan.root)
                    || self.inputs.borrow_return.is_some()
                    || self.inputs.carrier_return.is_some()
                {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                }
                Some(selected)
            } else {
                None
            };
            self.finish_return(result, unit_return, span)?;
            if receipt.is_some() {
                self.owner_plan
                    .as_mut()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                    .stage = 3;
                #[cfg(test)]
                if super::super::owner_consumer_test_support::take_fault(
                    super::super::owner_consumer_test_support::OwnerFault::DuplicateOwnerReturn,
                ) {
                    return self.owner_return(expression, result, unit_return, span);
                }
            }
            #[cfg(test)]
            super::super::super::record_owner_return(
                identity,
                result.map(|s| s.0),
                receipt.map(|s| s.0),
            );
            Ok(())
        }
    }
}
