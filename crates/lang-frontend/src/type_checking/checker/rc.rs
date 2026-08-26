use crate::{
    ast::{ExpressionId, TypeRefId},
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        ExpressionCategory, FunctionParameterType, IntrinsicTypeConstructor, RcOperationDescriptor,
        RcOperationKind, TypeId, TypeKind,
    },
};

use super::{Checker, ExprCheck, TypeCheckingError};

impl Checker<'_> {
    pub(super) fn check_rc_share_call(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let callee_node = self.ast().expressions().get(callee)?;
        let Expression::Member {
            receiver,
            name_span,
            safe,
            ..
        } = callee_node.payload().clone()
        else {
            return Ok(None);
        };
        if safe || self.sources.slice(name_span)? != "share" {
            return Ok(None);
        }
        let receiver_result = self.check_expression(receiver, None, None)?;
        let Some(payload_type) = self.rc_payload_type(receiver_result.ty) else {
            return Ok(None);
        };
        if !type_arguments.is_empty() {
            self.emit(
                self.type_argument_arity_code,
                "Rc.share accepts no type arguments",
                name_span,
            )?;
            self.check_construction_operands(arguments)?;
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through,
            }));
        }
        let mapping = self.map_arguments(&[], arguments, call_span)?;
        if let Err(error) = mapping {
            self.emit_mapping_error(error)?;
            self.check_construction_operands(arguments)?;
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through,
            }));
        }
        let return_type = receiver_result.ty;
        let function = self.types.intern(TypeKind::Function {
            move_only: false,
            parameters: Vec::<FunctionParameterType>::new(),
            return_type,
        });
        self.set_expression(callee, function);
        self.set_expression_category(callee, ExpressionCategory::Temporary);
        self.rc_operations.push(RcOperationDescriptor::new(
            expression,
            receiver,
            payload_type,
            RcOperationKind::Share,
        ));
        Ok(Some(ExprCheck {
            ty: return_type,
            falls_through: receiver_result.falls_through,
        }))
    }

    pub(super) fn rc_member_type(
        &mut self,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_type: TypeId,
        name: &str,
        safe: bool,
    ) -> Option<TypeId> {
        if safe || name != "value" {
            return None;
        }
        let payload_type = self.rc_payload_type(receiver_type)?;
        self.rc_operations.push(RcOperationDescriptor::new(
            expression,
            receiver,
            payload_type,
            RcOperationKind::Value,
        ));
        Some(payload_type)
    }

    fn rc_payload_type(&self, ty: TypeId) -> Option<TypeId> {
        let TypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Rc,
            arguments,
        } = self.kind(ty)
        else {
            return None;
        };
        match arguments.as_slice() {
            [payload] => Some(*payload),
            _ => None,
        }
    }
}
