//! One checked-scalar CFG algorithm; adapters only read and restore their transport roles.
use super::LoweredValue;
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{
        BlockId, CheckedArithmeticOperator, Edge, EntityId, EntityType, Function, Operation,
        Origin, SsaTypeId, TerminatorKind, ValueId,
    },
};
use lang_frontend::source::Span;

pub(in crate::ssa) struct CarryColumn {
    pub(in crate::ssa) source: EntityId,
    pub(in crate::ssa) ty: EntityType,
}

pub(in crate::ssa) struct CheckedCarry<R> {
    pub(in crate::ssa) columns: Vec<CarryColumn>,
    pub(in crate::ssa) roles: R,
}

/// No adapter port appends instructions, creates edges or selects a scalar result.
pub(in crate::ssa) trait CheckedLowering {
    type Roles;

    fn boolean_type(&self) -> Option<SsaTypeId>;
    fn ir_cursor(&mut self) -> (&mut Function, &mut BlockId);
    fn capture_roles(&self, span: Span) -> Result<CheckedCarry<Self::Roles>, LoweringError>;
    fn restore_roles(
        &mut self,
        roles: Self::Roles,
        from: BlockId,
        success: BlockId,
        span: Span,
    ) -> Result<(), LoweringError>;
}

pub(in crate::ssa) fn lower_checked_arithmetic<H: CheckedLowering>(
    host: &mut H,
    operator: CheckedArithmeticOperator,
    left: ValueId,
    right: ValueId,
    ty: SsaTypeId,
    span: Span,
) -> Result<LoweredValue, LoweringError> {
    lower_checked_operation(
        host,
        Operation::CheckedArithmetic {
            operator,
            left,
            right,
        },
        ty,
        span,
    )
}

pub(in crate::ssa) fn lower_checked_operation<H: CheckedLowering>(
    host: &mut H,
    operation: Operation,
    ty: SsaTypeId,
    span: Span,
) -> Result<LoweredValue, LoweringError> {
    let boolean = host
        .boolean_type()
        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
    let results = {
        let (function, block) = host.ir_cursor();
        function
            .append_instruction(
                *block,
                operation,
                vec![EntityType::Value(ty), EntityType::Value(boolean)],
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?
            .1
    };
    // Keep role validation after append, including its existing first-error order.
    let carried = host.capture_roles(span)?;
    let (from, success) = {
        let (function, block) = host.ir_cursor();
        let from = *block;
        let parameter_types = carried
            .columns
            .iter()
            .map(|column| column.ty)
            .collect::<Vec<_>>();
        let arguments = carried
            .columns
            .iter()
            .map(|column| column.source)
            .collect::<Vec<_>>();
        let failure = function
            .add_block(parameter_types.clone(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let success = function
            .add_block(parameter_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        function
            .set_terminator(
                from,
                TerminatorKind::Conditional {
                    condition: value(results[1], span)?,
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
        function
            .set_terminator(failure, TerminatorKind::Abort, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        *block = success;
        (from, success)
    };
    host.restore_roles(carried.roles, from, success, span)?;
    Ok(LoweredValue::Value(value(results[0], span)?))
}

// append_instruction allocates exactly the two requested Value entities above.
// Keep extraction at the original condition and final-delivery points.
fn value(entity: EntityId, span: Span) -> Result<ValueId, LoweringError> {
    match entity {
        EntityId::Value(value) => Ok(value),
        EntityId::Place(_) | EntityId::Loan(_) => Err(error(LoweringErrorKind::InvalidModel, span)),
    }
}
