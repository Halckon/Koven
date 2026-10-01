use crate::{
    ast::{ExpressionId, TypeRefId},
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        BuiltinType, ExpressionCategory, FunctionParameterType, StringOperationDescriptor, TypeKind,
    },
};

use super::{Checker, ExprCheck, TypeCheckingError};

impl Checker<'_> {
    pub(super) fn check_string_clone_call(
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
        if safe || self.sources.slice(name_span)? != "clone" {
            return Ok(None);
        }
        let receiver_result = self.check_expression(receiver, None, None)?;
        if !matches!(
            self.kind(receiver_result.ty),
            TypeKind::Builtin(BuiltinType::String)
        ) {
            if matches!(
                self.kind(receiver_result.ty),
                TypeKind::Builtin(_) | TypeKind::Intrinsic { .. } | TypeKind::Nullable(_)
            ) {
                self.emit(
                    self.unresolved_constant_code,
                    "receiver has no clone member",
                    name_span,
                )?;
                self.check_construction_operands(arguments)?;
                return Ok(Some(ExprCheck {
                    ty: self.error_type(),
                    falls_through: receiver_result.falls_through,
                }));
            }
            return Ok(None);
        }
        if !type_arguments.is_empty() {
            self.emit(
                self.type_argument_arity_code,
                "String.clone accepts no type arguments",
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
        self.string_operations.push(StringOperationDescriptor::new(
            expression,
            receiver,
            receiver_result.ty,
        ));
        Ok(Some(ExprCheck {
            ty: return_type,
            falls_through: receiver_result.falls_through,
        }))
    }
}
