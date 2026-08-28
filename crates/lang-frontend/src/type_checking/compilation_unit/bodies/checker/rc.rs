//! SPEC-0197 compilation-unit intrinsic `Rc<T>` member operations。

use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::codes,
    name_resolution::SourceUnitId,
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        CompilationUnitTypeError, ExpressionCategory, IntrinsicTypeConstructor, RcOperationKind,
        TypeCheckingError, UnitExpressionId, UnitFunctionParameterType, UnitRcOperationDescriptor,
        UnitTypeId, UnitTypeKind,
        argument_mapping::{MappedParameter, map_arguments},
    },
};

use super::{BodyChecker, ExpressionCheck};

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_rc_share_call(
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
        if self
            .sources
            .slice(name_span)
            .map_err(TypeCheckingError::from)?
            != "share"
        {
            return Ok(None);
        }
        let receiver_result = self.check_expression(source, receiver, None, None, return_type)?;
        let payload_type = match self.rc_payload_type(receiver_result.ty) {
            Some(_) if safe => return Err(CompilationUnitTypeError::UnsupportedBody(name_span)),
            Some(payload) => payload,
            None if self.nullable_rc_payload_type(receiver_result.ty).is_some() => {
                return Err(CompilationUnitTypeError::UnsupportedBody(name_span));
            }
            None => return Ok(None),
        };
        if self.construction_type_contains_poison(payload_type) {
            return Err(CompilationUnitTypeError::UnsupportedBody(name_span));
        }
        if !type_arguments.is_empty() {
            self.emit(
                codes::TYPE_ARGUMENT_ARITY,
                "Rc.share accepts no type arguments",
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
            .rc_operations
            .push(UnitRcOperationDescriptor::new(
                UnitExpressionId::new(source, expression),
                UnitExpressionId::new(source, receiver),
                payload_type,
                RcOperationKind::Share,
            ));
        Ok(Some(ExpressionCheck {
            ty: receiver_result.ty,
            falls_through: receiver_result.falls_through,
        }))
    }

    pub(super) fn rc_member_type(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_type: UnitTypeId,
        name_span: Span,
        safe: bool,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        if self
            .sources
            .slice(name_span)
            .map_err(TypeCheckingError::from)?
            != "value"
        {
            return Ok(None);
        }
        let payload_type = match self.rc_payload_type(receiver_type) {
            Some(_) if safe => {
                return Err(CompilationUnitTypeError::UnsupportedBody(name_span));
            }
            Some(payload) => payload,
            None if self.nullable_rc_payload_type(receiver_type).is_some() => {
                return Err(CompilationUnitTypeError::UnsupportedBody(name_span));
            }
            None => return Ok(None),
        };
        if self.construction_type_contains_poison(payload_type) {
            return Err(CompilationUnitTypeError::UnsupportedBody(name_span));
        }
        self.parts
            .rc_operations
            .push(UnitRcOperationDescriptor::new(
                UnitExpressionId::new(source, expression),
                UnitExpressionId::new(source, receiver),
                payload_type,
                RcOperationKind::Value,
            ));
        Ok(Some(payload_type))
    }

    fn rc_payload_type(&self, ty: UnitTypeId) -> Option<UnitTypeId> {
        let UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Rc,
            arguments,
        } = self.signatures.types().get(ty)?
        else {
            return None;
        };
        match arguments.as_slice() {
            [payload] => Some(*payload),
            _ => None,
        }
    }

    fn nullable_rc_payload_type(&self, ty: UnitTypeId) -> Option<UnitTypeId> {
        let UnitTypeKind::Nullable(inner) = self.signatures.types().get(ty)? else {
            return None;
        };
        self.rc_payload_type(*inner)
    }
}
