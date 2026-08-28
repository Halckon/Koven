//! SPEC-0197 compilation-unit postfix、cast 与 deferred reference traversal。

use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::codes,
    name_resolution::SourceUnitId,
    source::Span,
    type_checking::{DeferredReason, UnitTypeId, UnitTypeKind},
};

use super::{BodyChecker, CompilationUnitTypeError, ExpressionCheck};

impl BodyChecker<'_> {
    pub(super) fn check_super_member(
        &mut self,
        source: SourceUnitId,
        interface: TypeRefId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        self.resolve_static_body_type_ref(source, interface)?;
        Ok(ExpressionCheck {
            ty: self.deferred_type(DeferredReason::MemberAccess),
            falls_through: true,
        })
    }

    pub(super) fn check_non_null_assert(
        &mut self,
        source: SourceUnitId,
        operand: ExpressionId,
        operator_span: Span,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let operand = self.check_expression(source, operand, None, None, return_type)?;
        let ty = match self.signatures.types().get(operand.ty) {
            Some(UnitTypeKind::Nullable(inner)) => *inner,
            Some(UnitTypeKind::Error) => self.error_type(),
            Some(UnitTypeKind::Deferred(_)) => self.deferred_type(DeferredReason::MemberAccess),
            _ => {
                self.emit(
                    codes::INVALID_OPERAND_TYPES,
                    "non-null assertion requires a nullable operand",
                    operator_span,
                )?;
                self.error_type()
            }
        };
        Ok(ExpressionCheck {
            ty,
            falls_through: operand.falls_through,
        })
    }

    pub(super) fn check_cast(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        type_ref: TypeRefId,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let expression = self.check_expression(source, expression, None, None, return_type)?;
        self.resolve_body_type_ref(source, type_ref)?;
        Ok(ExpressionCheck {
            ty: self.deferred_type(DeferredReason::CastOrTypeTest),
            falls_through: expression.falls_through,
        })
    }

    pub(super) fn check_propagate(
        &mut self,
        source: SourceUnitId,
        value: ExpressionId,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let value = self.check_expression(source, value, None, None, return_type)?;
        Ok(ExpressionCheck {
            ty: self.deferred_type(DeferredReason::ErrorPropagation),
            falls_through: value.falls_through,
        })
    }

    pub(super) fn check_callable_reference(
        &mut self,
        source: SourceUnitId,
        receiver: Option<ExpressionId>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let falls_through = match receiver {
            Some(receiver) => {
                self.check_expression(source, receiver, None, None, return_type)?
                    .falls_through
            }
            None => true,
        };
        Ok(ExpressionCheck {
            ty: self.deferred_type(DeferredReason::OverloadSelection),
            falls_through,
        })
    }
}
