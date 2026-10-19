//! Checked scalar CFG preserves the existing entry-specific carry and rebind contract.
use super::{
    BuiltinType, CheckedArithmeticOperator, Edge, EntityType, ExpressionLowerer, LoweredValue,
    LoweringError, LoweringErrorKind, Operation, Origin, Span, SsaTypeId, TerminatorKind, ValueId,
    builtin_type, error, value,
};

impl ExpressionLowerer<'_> {
    pub(super) fn checked(
        &mut self,
        operator: CheckedArithmeticOperator,
        left: ValueId,
        right: ValueId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.checked_operation(
            Operation::CheckedArithmetic {
                operator,
                left,
                right,
            },
            ty,
            span,
        )
    }

    pub(super) fn checked_operation(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let boolean = self
            .type_ids
            .iter()
            .find_map(|(frontend, ssa)| {
                (builtin_type(self.typed, *frontend) == Some(BuiltinType::Boolean)).then_some(*ssa)
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (_, results) = self.append(
            operation,
            vec![EntityType::Value(ty), EntityType::Value(boolean)],
            span,
        )?;
        let baseline = self.bindings.clone();
        let carried = self.linear_binding_slots(&baseline, span)?;
        self.carry_checked_map(&carried, span)?;
        let previous_block = self.block;
        let parameter_types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
        let arguments = carried
            .slots
            .iter()
            .map(|slot| slot.source)
            .collect::<Vec<_>>();
        let failure = self
            .function
            .add_block(parameter_types.clone(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let success = self
            .function
            .add_block(parameter_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: value(results[1]),
                    when_true: Edge {
                        target: failure,
                        arguments: arguments.clone(),
                    },
                    when_false: Edge {
                        target: success,
                        arguments,
                    },
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(failure, TerminatorKind::Abort, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.block = success;
        self.bindings = self.rebind_linear_bindings(&baseline, success, &carried, span)?;
        self.resume_checked_map(previous_block, &carried, span)?;
        Ok(LoweredValue::Value(value(results[0])))
    }
}
