//! Runtime constructor evaluation order and synchronous borrowed generation.

use super::*;
use crate::ssa::model::{ComparisonOperator, Edge, Origin, PlaceAccess, TerminatorKind};
use lang_frontend::type_checking::ParameterMode;

impl ExpressionLowerer<'_> {
    pub(super) fn lower_runtime_container(
        &mut self,
        expression: ExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .container_construction(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let [size, initializer] = arguments else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if descriptor.parameter_modes() != [ParameterMode::Borrow, ParameterMode::Borrow] {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if self.runtime_operand_exits(size.value, size.span)? {
            return Ok(LoweredValue::Diverged);
        }
        // The first Borrow is live before the negative guard and any initializer expression.
        let (size_loan, ends_after_call) =
            self.lower_borrow_argument(expression, size.value, size.span)?;
        self.pending_call_loans.insert(
            (expression.index(), size.value.index()),
            ends_after_call.then_some(size_loan),
        );
        let size_type = self.expression_ssa_type(size.value, size.span)?;
        let (_, results) = self.append(
            Operation::Read {
                source: PlaceAccess::Loan(size_loan),
            },
            vec![EntityType::Value(size_type)],
            size.span,
        )?;
        let length = value(results[0]);
        self.guard_nonnegative_length(length, size_type, span)?;
        if self.runtime_operand_exits(initializer.value, initializer.span)? {
            return Ok(LoweredValue::Diverged);
        }
        // Successful frontend diagnostics do not resolve an overload operand. Its deferred
        // identity cannot choose an address or environment in the backend.
        let initializer_type = self
            .typed
            .expression_type(initializer.value)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, initializer.span))?;
        if matches!(
            self.typed.types().get(initializer_type),
            Some(lang_frontend::type_checking::TypeKind::Deferred(_))
        ) {
            return Err(error(LoweringErrorKind::UnsupportedNode, initializer.span));
        }
        let (initializer_loan, ends_after_call) =
            self.lower_borrow_argument(expression, initializer.value, initializer.span)?;
        self.pending_call_loans.insert(
            (expression.index(), initializer.value.index()),
            ends_after_call.then_some(initializer_loan),
        );
        self.validate_container_identity(
            descriptor.container(),
            descriptor.element_type(),
            descriptor.container_type(),
            span,
        )?;
        let container = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::ContainerGenerateBorrowed {
                container,
                length,
                initializer: initializer_loan,
            },
            vec![EntityType::Value(container)],
            span,
        )?;
        self.finish_borrowed_call(expression, span)?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    /// Nothing ends the operand prefix; its loan and later operand facts do not exist.
    fn runtime_operand_exits(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let ty = self
            .typed
            .expression_type(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if builtin_type(self.typed, ty) != Some(BuiltinType::Nothing) {
            return Ok(false);
        }
        match self.lower(expression)? {
            LoweredValue::Diverged => Ok(true),
            _ => Err(error(LoweringErrorKind::InvalidModel, span)),
        }
    }

    /// Reuse the existing linear carrier so loans and owners remain synchronized across the guard.
    fn guard_nonnegative_length(
        &mut self,
        length: crate::ssa::model::ValueId,
        int: SsaTypeId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let boolean = self
            .type_ids
            .iter()
            .find_map(|(frontend, ssa)| {
                (builtin_type(self.typed, *frontend) == Some(BuiltinType::Boolean)).then_some(*ssa)
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (_, zero) = self.append(
            Operation::Constant(ScalarConstant::Integer(0)),
            vec![EntityType::Value(int)],
            span,
        )?;
        let (_, negative) = self.append(
            Operation::Compare {
                operator: ComparisonOperator::LessThan,
                left: length,
                right: value(zero[0]),
            },
            vec![EntityType::Value(boolean)],
            span,
        )?;
        let baseline = self.bindings.clone();
        let carried = self.linear_binding_slots(&baseline, span)?;
        let parameter_types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
        let arguments = carried
            .slots
            .iter()
            .map(|slot| slot.source)
            .collect::<Vec<_>>();
        let abort = self
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
                    condition: value(negative[0]),
                    when_true: Edge {
                        target: abort,
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
            .set_terminator(abort, TerminatorKind::Abort, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.block = success;
        self.bindings = self.rebind_linear_bindings(&baseline, success, &carried, span)?;
        Ok(())
    }
}
