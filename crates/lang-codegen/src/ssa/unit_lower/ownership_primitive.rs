//! SPEC-0244：经 typed/ownership 双重事实授权的 owned mutable root 原子提交。

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{
        LoanKind as FrontendLoanKind, OwnershipPrimitiveValueTransfer, UnitDropPoint,
        UnitLoanTarget, UnitValueDeliveryKind,
    },
    parser::CallArgument,
    source::Span,
    type_checking::{
        BuiltinType, Copyability, OwnershipPrimitiveKind, ParameterMode, UnitCallDescriptor,
        UnitExpressionId, UnitOwnershipPrimitiveDescriptor,
    },
};

use super::{
    LoweredValue, UnitExpressionLowerer, builtin_type, call_lifetimes::PendingCallFrame,
    lowering_error, require_value, resolve_concrete_type, type_lower::is_supported_storage_type,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, LoanId, LoanKind, Operation, Origin, SsaTypeId, ValueId},
};

struct PendingRoot {
    symbol: UnitSymbolId,
    owner_slot: usize,
    loan_slot: usize,
    unit_binding: bool,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_ownership_primitive(
        &mut self,
        expression: ExpressionId,
        arguments: &[CallArgument],
        call: &UnitCallDescriptor,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let pending_start = self.pending_operands.len();
        self.pending_call_frames.push(PendingCallFrame {
            loop_depth: self.loops.len(),
            pending_start,
            exclusive_root_owners: Vec::new(),
            created_loans: Vec::new(),
        });
        let result = self.lower_ownership_primitive_in_frame(expression, arguments, call, span);
        self.pending_call_frames.pop();
        self.pending_operands.truncate(pending_start);
        result
    }

    fn lower_ownership_primitive_in_frame(
        &mut self,
        expression: ExpressionId,
        arguments: &[CallArgument],
        call: &UnitCallDescriptor,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let expression = UnitExpressionId::new(self.source_unit, expression);
        let primitive = self
            .typed
            .ownership_primitive(expression)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if primitive.expression() != expression
            || arguments.len() != 2
            || call.arguments().len() != 2
            || call.receiver().is_some()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let modes = match primitive.kind() {
            OwnershipPrimitiveKind::Replace => [ParameterMode::Inout, ParameterMode::Value],
            OwnershipPrimitiveKind::Swap => [ParameterMode::Inout, ParameterMode::Inout],
        };
        for (index, argument) in arguments.iter().enumerate() {
            let mapping = call
                .arguments()
                .iter()
                .find(|mapping| mapping.argument_index() == index)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            if primitive.operands()[index]
                != UnitExpressionId::new(self.source_unit, argument.value)
                || mapping.parameter_index() != index
                || mapping.mode() != modes[index]
                || mapping.parameter_type() != primitive.value_type()
            {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            }
        }
        let concrete = resolve_concrete_type(
            self.typed,
            primitive.value_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        if !is_supported_storage_type(self.typed, concrete) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let target = self
            .type_ids
            .get(&concrete)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let copyable = self.typed.copyability(concrete) == Copyability::Copyable;
        let unit = builtin_type(self.typed, concrete) == Some(BuiltinType::Unit);
        let mut roots = Vec::new();
        let mut replacement = None;
        for (index, argument) in arguments.iter().enumerate() {
            // Nothing 保留静态 descriptor，但不能要求不存在的正常 commit plan。
            if self.expression_builtin_type(argument.value, argument.span)?
                == Some(BuiltinType::Nothing)
            {
                return match self.lower(argument.value)? {
                    LoweredValue::Diverged => Ok(LoweredValue::Diverged),
                    _ => Err(lowering_error(
                        LoweringErrorKind::InvalidModel,
                        argument.span,
                    )),
                };
            }
            if modes[index] == ParameterMode::Inout {
                roots
                    .push(self.begin_primitive_root(expression, argument, target, copyable, span)?);
            } else {
                replacement = match self.lower_value_argument(
                    expression,
                    argument.value,
                    primitive.value_type(),
                    argument.span,
                )? {
                    LoweredValue::Value(value) => Some(if copyable {
                        // Copyable Value delivery 必须是独立 snapshot，而非目标 root 的 SSA alias。
                        self.copy_primitive_value(value, target, argument.span)?
                    } else {
                        value
                    }),
                    LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                    LoweredValue::Unit => {
                        return Err(lowering_error(
                            LoweringErrorKind::InvalidModel,
                            argument.span,
                        ));
                    }
                };
            }
        }
        self.require_primitive_commit(primitive, &roots, span)?;
        let operation = match (primitive.kind(), roots.as_slice(), replacement) {
            (OwnershipPrimitiveKind::Replace, [root], Some(replacement)) => {
                let (owner, loan) = self.primitive_root_entities(root, span)?;
                Operation::RootReplace {
                    owner,
                    loan,
                    replacement,
                }
            }
            (OwnershipPrimitiveKind::Swap, [left, right], None) => {
                let (left_owner, left_loan) = self.primitive_root_entities(left, span)?;
                let (right_owner, right_loan) = self.primitive_root_entities(right, span)?;
                Operation::RootSwap {
                    owners: [left_owner, right_owner],
                    loans: [left_loan, right_loan],
                }
            }
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                operation,
                vec![EntityType::Value(target); 2],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        for (root, result) in roots.iter().zip(results.iter()) {
            self.bindings.insert(
                root.symbol,
                if root.unit_binding {
                    LoweredValue::Unit
                } else {
                    LoweredValue::Value(require_value(*result, span)?)
                },
            );
        }
        // Commit 自身消费 exclusive loans 与旧 owner；不得再发出 BorrowEnd 或 drop 旧值。
        self.emit_borrow_argument_expression_drops(expression)?;
        self.emit_drops(UnitDropPoint::CallReturn(expression))?;
        match primitive.kind() {
            OwnershipPrimitiveKind::Replace if !unit => {
                Ok(LoweredValue::Value(require_value(results[1], span)?))
            }
            OwnershipPrimitiveKind::Replace => Ok(LoweredValue::Unit),
            OwnershipPrimitiveKind::Swap => Ok(LoweredValue::Unit),
        }
    }

    fn begin_primitive_root(
        &mut self,
        call: UnitExpressionId,
        argument: &CallArgument,
        target: SsaTypeId,
        copyable: bool,
        span: Span,
    ) -> Result<PendingRoot, LoweringError> {
        let symbol = self.direct_place_symbol(argument.value, argument.span)?;
        let mut loans = self.owned.loans().iter().filter(|loan| {
            loan.call() == call
                && loan.argument() == UnitExpressionId::new(self.source_unit, argument.value)
        });
        let fact = loans
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
        if loans.next().is_some()
            || fact.kind() != FrontendLoanKind::Exclusive
            || fact.end_span() != span
        {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                argument.span,
            ));
        }
        match fact.target() {
            UnitLoanTarget::Place(place) if place.is_root() && place.root() == symbol => {}
            UnitLoanTarget::Place(_) | UnitLoanTarget::Temporary(_) | UnitLoanTarget::This(_) => {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    argument.span,
                ));
            }
        }
        let unit_binding = self.bindings.get(&symbol).copied() == Some(LoweredValue::Unit);
        let mut owner = match self.bindings.get(&symbol).copied() {
            Some(LoweredValue::Value(owner)) => owner,
            Some(LoweredValue::Unit) => {
                // Unit 局部绑定平时不占实体；原子 place 使用时才物化零大小 owned 值。
                // 保留源 binding 的 Unit 表示；仅 pending owner/loan 需要真实实体。
                self.materialize_unit_value(argument.value, argument.span)?
            }
            // Borrow / Inout 参数没有 root owner；第一片不承接其非 owning place。
            Some(LoweredValue::Diverged) | None => {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    argument.span,
                ));
            }
        };
        if self
            .function
            .entity(EntityId::Value(owner))
            .map(|entity| entity.ty)
            != Some(EntityType::Value(target))
        {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                argument.span,
            ));
        }
        if copyable {
            // 不同 Copyable bindings 可能共享原 SSA 值，但它们拥有不重叠的 mutable storage。
            owner = self.copy_primitive_value(owner, target, argument.span)?;
            if !unit_binding {
                self.bindings.insert(symbol, LoweredValue::Value(owner));
            }
        }
        let (_, places) = self
            .function
            .append_instruction(
                self.block,
                Operation::RootPlace { owner },
                vec![EntityType::Place(target)],
                Origin::Source(fact.begin_span()),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, argument.span))?;
        let EntityId::Place(place) = places[0] else {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                argument.span,
            ));
        };
        let (_, loans) = self
            .function
            .append_instruction(
                self.block,
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Exclusive,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target,
                }],
                Origin::Source(fact.begin_span()),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, argument.span))?;
        let EntityId::Loan(loan) = loans[0] else {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                argument.span,
            ));
        };
        let owner_slot = self.pending_operands.len();
        self.pending_operands.push(EntityId::Value(owner));
        let loan_slot = self.pending_operands.len();
        self.pending_operands.push(EntityId::Loan(loan));
        let frame = self
            .pending_call_frames
            .last_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, argument.span))?;
        frame.created_loans.push(loan_slot);
        frame.exclusive_root_owners.push(owner_slot);
        Ok(PendingRoot {
            symbol,
            owner_slot,
            loan_slot,
            unit_binding,
        })
    }

    fn primitive_root_entities(
        &self,
        root: &PendingRoot,
        span: Span,
    ) -> Result<(ValueId, LoanId), LoweringError> {
        let Some(EntityId::Value(owner)) = self.pending_operands.get(root.owner_slot).copied()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some(EntityId::Loan(loan)) = self.pending_operands.get(root.loan_slot).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let binding = if root.unit_binding {
            LoweredValue::Unit
        } else {
            LoweredValue::Value(owner)
        };
        if self.bindings.get(&root.symbol).copied() != Some(binding) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok((owner, loan))
    }

    fn require_primitive_commit(
        &self,
        primitive: UnitOwnershipPrimitiveDescriptor,
        roots: &[PendingRoot],
        span: Span,
    ) -> Result<(), LoweringError> {
        let plan = self
            .owned
            .ownership_primitive(primitive.expression())
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        if plan.descriptor() != &primitive
            || plan.places().len() != roots.len()
            || plan
                .places()
                .iter()
                .zip(roots)
                .any(|(place, root)| !place.is_root() || place.root() != root.symbol)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let expected = match primitive.kind() {
            OwnershipPrimitiveKind::Swap => None,
            OwnershipPrimitiveKind::Replace => {
                let delivery = self
                    .owned
                    .value_deliveries()
                    .iter()
                    .find(|delivery| {
                        delivery.call() == primitive.expression()
                            && delivery.argument() == primitive.operands()[1]
                    })
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                Some(match delivery.kind() {
                    UnitValueDeliveryKind::Copy => OwnershipPrimitiveValueTransfer::Copy,
                    UnitValueDeliveryKind::Move => OwnershipPrimitiveValueTransfer::Move,
                    UnitValueDeliveryKind::Temporary => OwnershipPrimitiveValueTransfer::Temporary,
                })
            }
        };
        if plan.new_value_transfer() != expected {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    fn copy_primitive_value(
        &mut self,
        value: ValueId,
        target: SsaTypeId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let (_, values) = self
            .function
            .append_instruction(
                self.block,
                Operation::Copy { source: value },
                vec![EntityType::Value(target)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        require_value(values[0], span)
    }
}
