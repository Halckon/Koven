//! Compilation-unit Map factory admission and fact publication.
use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::codes,
    name_resolution::SourceUnitId,
    parser::CallArgument,
    source::Span,
    type_checking::{
        CompilationUnitTypeError, IntrinsicCallable, IntrinsicTypeConstructor, TypeCheckingError,
        UnitExpressionId, UnitMapConstructionDescriptor, UnitTypeId, UnitTypeKind,
    },
};

use super::{BodyChecker, ExpressionCheck};

impl BodyChecker<'_> {
    /// 检查多文件编译单元中 `mapOf()` 与 `mutableMapOf()` 构造调用。
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn check_map_construction_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        call_span: Span,
        _callee: ExpressionId,
        callable: IntrinsicCallable,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        expected: Option<UnitTypeId>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let target_constructor = match callable {
            IntrinsicCallable::MapOf => IntrinsicTypeConstructor::Map,
            IntrinsicCallable::MutableMapOf => IntrinsicTypeConstructor::MutableMap,
            _ => unreachable!(),
        };

        let (key_type, value_type) = if type_arguments.len() == 2 {
            let key = self.resolve_body_type_ref(source, type_arguments[0])?;
            let val = self.resolve_body_type_ref(source, type_arguments[1])?;
            (key, val)
        } else if type_arguments.is_empty() {
            if let Some(expected_ty) = expected
                && let Some((expected_ctor, k, v)) = self.map_parts(expected_ty)
                && expected_ctor == target_constructor
            {
                (k, v)
            } else {
                self.emit(
                    codes::CANNOT_INFER_CONTAINER_ELEMENT,
                    "cannot infer Map key and value types without type arguments or contextual type",
                    call_span,
                )?;
                self.check_construction_operands(source, arguments, return_type)?;
                return Ok(ExpressionCheck {
                    ty: self.error_type(),
                    falls_through: true,
                });
            }
        } else {
            self.emit(
                codes::TYPE_ARGUMENT_ARITY,
                "map construction accepts exactly two type arguments",
                call_span,
            )?;
            self.check_rejected_construction_operands(
                source,
                type_arguments,
                arguments,
                return_type,
            )?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        };

        if self.is_error(key_type) || self.is_error(value_type) {
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        if !self.is_hashable_type(key_type)
            && !self.unbound_intrinsic_argument_head(
                source,
                type_arguments.first().copied(),
                key_type,
            )?
        {
            let key_span = if !type_arguments.is_empty() {
                self.file(source)
                    .ast()
                    .type_refs()
                    .get(type_arguments[0])
                    .map_err(TypeCheckingError::from)?
                    .span()
            } else {
                call_span
            };
            self.emit(
                codes::HASHABLE_TYPE_ARGUMENT_BOUND,
                "Map key type must be Hashable",
                key_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        if !self.could_be_structurally_storable_type(value_type) {
            let val_span = if !type_arguments.is_empty() {
                self.file(source)
                    .ast()
                    .type_refs()
                    .get(type_arguments[1])
                    .map_err(TypeCheckingError::from)?
                    .span()
            } else {
                call_span
            };
            self.emit(
                codes::INVALID_CONTAINER_ELEMENT,
                "container value type is not structurally storable",
                val_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        if !arguments.is_empty() {
            self.emit(
                codes::CALL_ARGUMENT_ARITY,
                "mapOf and mutableMapOf do not accept arguments in v1",
                call_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(self.failed_construction());
        }

        if self.contains_unresolved_call_type(key_type)
            || self.contains_unresolved_call_type(value_type)
        {
            return Ok(self.deferred_container_construction(
                UnitExpressionId::new(source, expression),
                target_constructor,
                vec![key_type, value_type],
            ));
        }

        let map_ty = self.signatures.types_mut().intern(UnitTypeKind::Intrinsic {
            constructor: target_constructor,
            arguments: vec![key_type, value_type],
        });

        self.parts
            .map_descriptors
            .constructions
            .push(UnitMapConstructionDescriptor::new(
                UnitExpressionId::new(source, expression),
                callable,
                map_ty,
                key_type,
                value_type,
            ));

        Ok(ExpressionCheck {
            ty: map_ty,
            falls_through: true,
        })
    }
}
