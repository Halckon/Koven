//! Compilation-unit builtin 整数按位取反。

use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::codes,
    name_resolution::SourceUnitId,
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, ExpressionCategory, TypeCheckingError,
        UnitExpressionId, UnitFunctionParameterType, UnitIntegerOperationDescriptor, UnitTypeId,
        UnitTypeKind,
        argument_mapping::{MappedParameter, map_arguments},
    },
};

use super::{BodyChecker, ExpressionCheck};

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_integer_inv_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        return_type: UnitTypeId,
    ) -> Result<Option<ExpressionCheck>, CompilationUnitTypeError> {
        let Expression::Member {
            receiver,
            name_span,
            safe,
            ..
        } = self
            .file(source)
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?
            .payload()
            .clone()
        else {
            return Ok(None);
        };
        if safe
            || self
                .sources
                .slice(name_span)
                .map_err(TypeCheckingError::from)?
                != "inv"
        {
            return Ok(None);
        }
        let receiver_result = self.check_expression(source, receiver, None, None, return_type)?;
        if !matches!(
            self.signatures.types().get(receiver_result.ty),
            Some(UnitTypeKind::Builtin(
                BuiltinType::Byte
                    | BuiltinType::Short
                    | BuiltinType::Int
                    | BuiltinType::Long
                    | BuiltinType::UByte
                    | BuiltinType::UShort
                    | BuiltinType::UInt
                    | BuiltinType::ULong
            ))
        ) {
            if matches!(
                self.signatures.types().get(receiver_result.ty),
                Some(
                    UnitTypeKind::Builtin(_)
                        | UnitTypeKind::Intrinsic { .. }
                        | UnitTypeKind::Nullable(_)
                )
            ) {
                self.emit(
                    codes::UNRESOLVED_NAME,
                    "receiver has no inv member",
                    name_span,
                )?;
                self.check_call_arguments_without_expected(source, arguments, return_type)?;
                return Ok(Some(ExpressionCheck {
                    ty: self.error_type(),
                    falls_through: receiver_result.falls_through,
                }));
            }
            return Ok(None);
        }
        if !type_arguments.is_empty() {
            self.emit(
                codes::TYPE_ARGUMENT_ARITY,
                "integer inv accepts no type arguments",
                name_span,
            )?;
            self.check_call_arguments_without_expected(source, arguments, return_type)?;
            return Ok(Some(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through,
            }));
        }
        let parameters = Vec::<MappedParameter<UnitTypeId>>::new();
        match map_arguments(self.sources, &parameters, arguments, call_span, |_| {
            Ok(false)
        })? {
            Ok(_) => {}
            Err(error) => {
                self.emit_mapping_error(error)?;
                self.check_call_arguments_without_expected(source, arguments, return_type)?;
                return Ok(Some(ExpressionCheck {
                    ty: self.error_type(),
                    falls_through: receiver_result.falls_through,
                }));
            }
        }
        let function = self.signatures.types_mut().intern(UnitTypeKind::Function {
            move_only: false,
            parameters: Vec::<UnitFunctionParameterType>::new(),
            return_type: receiver_result.ty,
        });
        self.record_expression(source, callee, function);
        self.parts.expression_categories.insert(
            UnitExpressionId::new(source, callee),
            ExpressionCategory::Temporary,
        );
        self.parts
            .integer_operations
            .push(UnitIntegerOperationDescriptor::new(
                UnitExpressionId::new(source, expression),
                UnitExpressionId::new(source, receiver),
                receiver_result.ty,
            ));
        Ok(Some(ExpressionCheck {
            ty: receiver_result.ty,
            falls_through: receiver_result.falls_through,
        }))
    }
}
