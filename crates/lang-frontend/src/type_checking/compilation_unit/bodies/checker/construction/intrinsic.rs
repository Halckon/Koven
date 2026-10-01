//! compilation-unit compiler-bound `Box` / `Rc` construction。

use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::codes,
    name_resolution::SourceUnitId,
    parser::CallArgument,
    source::Span,
    type_checking::{
        CompilationUnitTypeError, IntrinsicTypeConstructor, NominalKind, ParameterMode,
        TypeCheckingError, UnitConstructionArgumentDescriptor, UnitConstructionDescriptor,
        UnitConstructionInstanceKey, UnitConstructionTarget, UnitExpressionId, UnitTypeId,
        UnitTypeKind,
        argument_mapping::{MappedParameter, map_arguments},
    },
};

use super::{BodyChecker, ExpressionCheck, Target};

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_intrinsic_construction(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        call_span: Span,
        callee_span: Span,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        target: Target,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let (parameter_name, construction_target, constructor) = match target {
            Target::Box => (
                "element",
                UnitConstructionTarget::IntrinsicBox,
                IntrinsicTypeConstructor::Box,
            ),
            Target::Rc => (
                "value",
                UnitConstructionTarget::IntrinsicRc,
                IntrinsicTypeConstructor::Rc,
            ),
            Target::Nominal(_) | Target::EnumCase(_) | Target::Invalid => {
                unreachable!("source or invalid target passed as intrinsic construction")
            }
        };
        let parameters = [MappedParameter {
            name: Some(parameter_name.to_owned()),
            mode: ParameterMode::Value,
            ty: self.error_type(),
            span: None,
        }];
        let mapping = match map_arguments(
            self.sources,
            &parameters,
            arguments,
            call_span,
            |argument| Ok(self.is_syntactic_place(source, argument)),
        )? {
            Ok(mapping) => mapping,
            Err(error) => {
                self.emit_mapping_error(error)?;
                self.check_construction_operands(source, arguments, return_type)?;
                return Ok(self.failed_construction());
            }
        };
        let operand = arguments[0].value;
        let payload = match type_arguments {
            [] => {
                self.check_expression(source, operand, None, None, return_type)?
                    .ty
            }
            [type_ref] => self.resolve_body_type_ref(source, *type_ref)?,
            _ => {
                let message = if matches!(target, Target::Box) {
                    "Box constructor accepts exactly one type argument"
                } else {
                    "Rc constructor accepts at most one type argument"
                };
                self.emit(codes::TYPE_ARGUMENT_ARITY, message, callee_span)?;
                self.check_construction_operands(source, arguments, return_type)?;
                return Ok(self.failed_construction());
            }
        };
        if self.construction_type_contains_poison(payload) {
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(self.failed_construction());
        }
        let operand_span = self
            .file(source)
            .ast()
            .expressions()
            .get(operand)
            .map_err(TypeCheckingError::from)?
            .span();
        if matches!(target, Target::Box) && !self.is_concrete_boxable(payload) {
            self.emit(
                codes::INVALID_BOX_ARGUMENT,
                "Box type argument must be a concrete value class or enum class instance",
                operand_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(self.failed_construction());
        }
        if matches!(target, Target::Rc) && !self.is_structurally_storable_type(payload) {
            self.emit(
                codes::INVALID_CONTAINER_ELEMENT,
                "Rc payload type is not structurally storable",
                operand_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(self.failed_construction());
        }
        let result_type = self.signatures.types_mut().intern(UnitTypeKind::Intrinsic {
            constructor,
            arguments: vec![payload],
        });
        self.finish_intrinsic_construction(
            source,
            expression,
            construction_target,
            parameter_name,
            payload,
            result_type,
            arguments,
            &mapping,
            expected,
            expected_span,
            return_type,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_intrinsic_construction(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        target: UnitConstructionTarget,
        parameter_name: &str,
        parameter_type: UnitTypeId,
        result_type: UnitTypeId,
        arguments: &[CallArgument],
        mapping: &[usize],
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let argument = &arguments[0];
        let argument_key = UnitExpressionId::new(source, argument.value);
        let already_checked = self.parts.expression_types.contains_key(&argument_key);
        let checked = self.check_expression(
            source,
            argument.value,
            Some(parameter_type),
            None,
            return_type,
        )?;
        if already_checked
            && !self.is_error(checked.ty)
            && !self.is_deferred(checked.ty)
            && !self.assignable(checked.ty, parameter_type)
        {
            let primary = self
                .file(source)
                .ast()
                .expressions()
                .get(argument.value)
                .map_err(TypeCheckingError::from)?
                .span();
            self.emit(
                codes::TYPE_MISMATCH,
                "expression type does not match the expected type",
                primary,
            )?;
            return Ok(self.failed_construction());
        }
        if self.construction_type_contains_poison(checked.ty) {
            return Ok(self.failed_construction());
        }
        let expression_span = self
            .file(source)
            .ast()
            .expressions()
            .get(expression)
            .map_err(TypeCheckingError::from)?
            .span();
        if !self.validate_construction_result(
            result_type,
            expected,
            expression_span,
            expected_span,
        )? {
            return Ok(self.failed_construction());
        }
        let parameter_index = mapping[0];
        self.parts.constructions.push(UnitConstructionDescriptor {
            expression: UnitExpressionId::new(source, expression),
            instance: UnitConstructionInstanceKey {
                target,
                type_arguments: vec![parameter_type],
            },
            result_type,
            arguments: vec![UnitConstructionArgumentDescriptor {
                parameter_index,
                parameter_symbol: None,
                parameter_name: parameter_name.to_owned(),
                parameter_type,
                argument: argument_key,
                evaluation_index: 0,
                category: self.expression_category(source, argument.value),
            }],
        });
        Ok(ExpressionCheck {
            ty: result_type,
            falls_through: true,
        })
    }

    fn is_concrete_boxable(&self, ty: UnitTypeId) -> bool {
        let Some(UnitTypeKind::Nominal { declaration, .. }) = self.signatures.types().get(ty)
        else {
            return false;
        };
        self.signatures
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .is_some_and(|nominal| {
                matches!(
                    nominal.kind(),
                    NominalKind::ValueClass | NominalKind::EnumClass
                )
            })
    }
}
