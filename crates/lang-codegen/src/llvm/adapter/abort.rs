//! SPEC-0347: abort reasons and source positions lowered to the runtime abort report.
use super::{FunctionLowerer, IntPredicate, LlvmAdapterError, unsupported};
use crate::{
    llvm::runtime::AbortMessage,
    ssa::model::{AbortReason, CheckedArithmeticOperator, EntityId, Module, Origin, SsaTypeId},
};

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    /// SPEC-0347: the source position printed by aborts emitted for this origin.
    pub(super) fn set_abort_location(&self, origin: &Origin) -> Result<(), LlvmAdapterError> {
        let location = match self.dependencies.debug {
            Some(debug) => Some(debug.abort_location(origin)?),
            None => None,
        };
        self.dependencies.runtime.set_abort_location(location);
        Ok(())
    }

    pub(super) fn lower_abort(
        &mut self,
        reason: Option<&AbortReason>,
    ) -> Result<(), LlvmAdapterError> {
        let runtime = self.dependencies.runtime;
        match reason {
            Some(AbortReason::Error { message }) => {
                let view = self.string_view(EntityId::Loan(*message))?;
                runtime.emit_abort(
                    &self.builder,
                    AbortMessage::Bytes {
                        bytes: view.bytes(),
                        length: view.length(),
                    },
                )
            }
            Some(AbortReason::CheckedArithmetic { operator, divisor }) => {
                let overflow = match operator {
                    CheckedArithmeticOperator::Add => "integer overflow in +",
                    CheckedArithmeticOperator::Subtract => "integer overflow in -",
                    CheckedArithmeticOperator::Multiply => "integer overflow in *",
                    CheckedArithmeticOperator::Divide => "integer overflow in /",
                    CheckedArithmeticOperator::Remainder => "integer overflow in %",
                };
                match operator {
                    CheckedArithmeticOperator::Divide | CheckedArithmeticOperator::Remainder => {
                        let divisor = self.int_value(*divisor)?;
                        let zero = self.builder.build_int_compare(
                            IntPredicate::EQ,
                            divisor,
                            divisor.get_type().const_zero(),
                            "abort.division_by_zero",
                        )?;
                        runtime.emit_abort(
                            &self.builder,
                            AbortMessage::Select {
                                condition: zero,
                                when_true: "division by zero",
                                when_false: overflow,
                            },
                        )
                    }
                    _ => runtime.emit_abort(&self.builder, AbortMessage::Text(overflow)),
                }
            }
            Some(AbortReason::CheckedConversion { target }) => {
                let text = format!(
                    "value out of range for {}",
                    integer_type_name(self.module, *target)?
                );
                runtime.emit_abort(&self.builder, AbortMessage::Text(&text))
            }
            Some(AbortReason::Text(text)) => {
                runtime.emit_abort(&self.builder, AbortMessage::Text(text))
            }
            None => runtime.emit_abort(&self.builder, AbortMessage::Text("abort")),
        }
    }
}

/// Koven name of a checked-conversion target for the SPEC-0347 abort message.
fn integer_type_name(module: &Module, ty: SsaTypeId) -> Result<&'static str, LlvmAdapterError> {
    match module.type_kind(ty) {
        Some(crate::ssa::model::SsaTypeKind::Integer { bits, signed }) => {
            Ok(match (*bits, *signed) {
                (8, true) => "Byte",
                (16, true) => "Short",
                (32, true) => "Int",
                (64, true) => "Long",
                (8, false) => "UByte",
                (16, false) => "UShort",
                (32, false) => "UInt",
                (64, false) => "ULong",
                _ => return Err(unsupported("checked conversion target width")),
            })
        }
        _ => Err(unsupported("checked conversion target is not an integer")),
    }
}
