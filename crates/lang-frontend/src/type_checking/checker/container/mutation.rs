//! MutableList mutation argument checking and descriptors.
use super::*;

impl Checker<'_> {
    fn prepare_mutable_list_call(
        &mut self,
        callee: ExpressionId,
        expected_name: &'static str,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<Option<(ExpressionId, TypeId, TypeId, bool)>, TypeCheckingError> {
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
        if safe || self.sources.slice(name_span)? != expected_name {
            return Ok(None);
        }
        let receiver_result = self.check_expression(receiver, None, None)?;
        let Some((container, element_type)) = self.container_parts(receiver_result.ty) else {
            return Ok(None);
        };
        if container != SequentialContainerKind::MutableList {
            self.emit(
                self.invalid_container_member_code,
                &format!("sequential container does not support '{expected_name}'"),
                name_span,
            )?;
            self.check_construction_operands(arguments)?;
            return Ok(Some((
                receiver,
                self.error_type(),
                self.error_type(),
                receiver_result.falls_through,
            )));
        }
        if !type_arguments.is_empty() {
            self.emit(
                self.type_argument_arity_code,
                &format!("MutableList.{expected_name} accepts no type arguments"),
                name_span,
            )?;
            self.check_construction_operands(arguments)?;
            return Ok(Some((
                receiver,
                self.error_type(),
                self.error_type(),
                receiver_result.falls_through,
            )));
        }
        Ok(Some((
            receiver,
            receiver_result.ty,
            element_type,
            receiver_result.falls_through,
        )))
    }

    pub(in crate::type_checking::checker) fn check_container_append_call(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let Some((receiver, receiver_ty, element_type, falls_through)) =
            self.prepare_mutable_list_call(callee, "add", type_arguments, arguments)?
        else {
            return Ok(None);
        };
        if receiver_ty == self.error_type() {
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through,
            }));
        }
        let parameters = [MappedParameter {
            name: None,
            mode: ParameterMode::Value,
            ty: element_type,
            span: None,
        }];
        let mapping = self.map_arguments(&parameters, arguments, call_span)?;
        if let Err(error) = mapping {
            self.emit_mapping_error(error)?;
            self.check_construction_operands(arguments)?;
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through,
            }));
        }
        let element_arg = &arguments[0];
        let element_result =
            self.check_pending_receiver_operand(receiver, element_arg.value, element_type)?;
        if !self.is_error(element_result.ty)
            && !self.is_deferred(element_result.ty)
            && !self.assignable(element_result.ty, element_type)
        {
            self.mismatch(
                self.ast().expressions().get(element_arg.value)?.span(),
                None,
                element_result.ty,
                element_type,
            )?;
        }
        let unit_type = self.builtin(BuiltinType::Unit);
        let function = self.types.intern(TypeKind::Function {
            move_only: false,
            parameters: vec![FunctionParameterType {
                mode: ParameterMode::Value,
                ty: element_type,
            }],
            return_type: unit_type,
        });
        self.set_expression(callee, function);
        self.set_expression_category(callee, ExpressionCategory::Temporary);
        self.container_appends.push(ContainerAppendDescriptor::new(
            expression,
            receiver,
            element_arg.value,
            receiver_ty,
            element_type,
            unit_type,
            call_span,
        ));
        Ok(Some(ExprCheck {
            ty: unit_type,
            falls_through: falls_through && element_result.falls_through,
        }))
    }

    pub(in crate::type_checking::checker) fn check_container_clear_call(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let Some((receiver, receiver_ty, element_type, falls_through)) =
            self.prepare_mutable_list_call(callee, "clear", type_arguments, arguments)?
        else {
            return Ok(None);
        };
        if receiver_ty == self.error_type() {
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through,
            }));
        }
        let parameters: [MappedParameter<TypeId>; 0] = [];
        let mapping = self.map_arguments(&parameters, arguments, call_span)?;
        if let Err(error) = mapping {
            self.emit_mapping_error(error)?;
            self.check_construction_operands(arguments)?;
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through,
            }));
        }
        let unit_type = self.builtin(BuiltinType::Unit);
        let function = self.types.intern(TypeKind::Function {
            move_only: false,
            parameters: Vec::new(),
            return_type: unit_type,
        });
        self.set_expression(callee, function);
        self.set_expression_category(callee, ExpressionCategory::Temporary);
        self.container_clears.push(ContainerClearDescriptor::new(
            expression,
            receiver,
            receiver_ty,
            element_type,
            unit_type,
            call_span,
        ));
        Ok(Some(ExprCheck {
            ty: unit_type,
            falls_through,
        }))
    }

    pub(in crate::type_checking::checker) fn check_container_remove_at_call(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let Some((receiver, receiver_ty, element_type, falls_through)) =
            self.prepare_mutable_list_call(callee, "removeAt", type_arguments, arguments)?
        else {
            return Ok(None);
        };
        if receiver_ty == self.error_type() {
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through,
            }));
        }
        let int_type = self.builtin(BuiltinType::Int);
        let parameters = [MappedParameter {
            name: None,
            mode: ParameterMode::Value,
            ty: int_type,
            span: None,
        }];
        let mapping = self.map_arguments(&parameters, arguments, call_span)?;
        if let Err(error) = mapping {
            self.emit_mapping_error(error)?;
            self.check_construction_operands(arguments)?;
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through,
            }));
        }
        let index_arg = &arguments[0];
        let index_result =
            self.check_pending_receiver_operand(receiver, index_arg.value, int_type)?;
        if !self.is_error(index_result.ty)
            && !self.is_deferred(index_result.ty)
            && !self.assignable(index_result.ty, int_type)
        {
            self.mismatch(
                self.ast().expressions().get(index_arg.value)?.span(),
                None,
                index_result.ty,
                int_type,
            )?;
        }
        let function = self.types.intern(TypeKind::Function {
            move_only: false,
            parameters: vec![FunctionParameterType {
                mode: ParameterMode::Value,
                ty: int_type,
            }],
            return_type: element_type,
        });
        self.set_expression(callee, function);
        self.set_expression_category(callee, ExpressionCategory::Temporary);
        self.container_remove_ats
            .push(ContainerRemoveAtDescriptor::new(
                expression,
                receiver,
                index_arg.value,
                receiver_ty,
                element_type,
                element_type,
                call_span,
            ));
        Ok(Some(ExprCheck {
            ty: element_type,
            falls_through: falls_through && index_result.falls_through,
        }))
    }

    fn check_container_endpoint_removal_call(
        &mut self,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        expected_name: &'static str,
    ) -> Result<Option<(ExpressionId, TypeId, TypeId, bool)>, TypeCheckingError> {
        let Some((receiver, receiver_ty, element_type, falls_through)) =
            self.prepare_mutable_list_call(callee, expected_name, type_arguments, arguments)?
        else {
            return Ok(None);
        };
        if receiver_ty == self.error_type() {
            return Ok(Some((
                receiver,
                self.error_type(),
                self.error_type(),
                falls_through,
            )));
        }
        let parameters: [MappedParameter<TypeId>; 0] = [];
        let mapping = self.map_arguments(&parameters, arguments, call_span)?;
        if let Err(error) = mapping {
            self.emit_mapping_error(error)?;
            self.check_construction_operands(arguments)?;
            return Ok(Some((
                receiver,
                self.error_type(),
                self.error_type(),
                falls_through,
            )));
        }
        let function = self.types.intern(TypeKind::Function {
            move_only: false,
            parameters: Vec::new(),
            return_type: element_type,
        });
        self.set_expression(callee, function);
        self.set_expression_category(callee, ExpressionCategory::Temporary);
        Ok(Some((receiver, receiver_ty, element_type, falls_through)))
    }

    pub(in crate::type_checking::checker) fn check_container_remove_last_call(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let Some((receiver, container_ty, element_type, falls_through)) = self
            .check_container_endpoint_removal_call(
                call_span,
                callee,
                type_arguments,
                arguments,
                "removeLast",
            )?
        else {
            return Ok(None);
        };
        if element_type != self.error_type() {
            self.container_remove_lasts
                .push(ContainerRemoveLastDescriptor::new(
                    expression,
                    receiver,
                    container_ty,
                    element_type,
                    element_type,
                    call_span,
                ));
        }
        Ok(Some(ExprCheck {
            ty: element_type,
            falls_through,
        }))
    }

    pub(in crate::type_checking::checker) fn check_container_remove_first_call(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let Some((receiver, container_ty, element_type, falls_through)) = self
            .check_container_endpoint_removal_call(
                call_span,
                callee,
                type_arguments,
                arguments,
                "removeFirst",
            )?
        else {
            return Ok(None);
        };
        if element_type != self.error_type() {
            self.container_remove_firsts
                .push(ContainerRemoveFirstDescriptor::new(
                    expression,
                    receiver,
                    container_ty,
                    element_type,
                    element_type,
                    call_span,
                ));
        }
        Ok(Some(ExprCheck {
            ty: element_type,
            falls_through,
        }))
    }

    pub(in crate::type_checking::checker) fn check_container_insert_at_call(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let Some((receiver, receiver_ty, element_type, falls_through)) =
            self.prepare_mutable_list_call(callee, "insertAt", type_arguments, arguments)?
        else {
            return Ok(None);
        };
        if receiver_ty == self.error_type() {
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through,
            }));
        }
        let int_type = self.builtin(BuiltinType::Int);
        let parameters = [
            MappedParameter {
                name: None,
                mode: ParameterMode::Value,
                ty: int_type,
                span: None,
            },
            MappedParameter {
                name: None,
                mode: ParameterMode::Value,
                ty: element_type,
                span: None,
            },
        ];
        let mapping = self.map_arguments(&parameters, arguments, call_span)?;
        if let Err(error) = mapping {
            self.emit_mapping_error(error)?;
            self.check_construction_operands(arguments)?;
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through,
            }));
        }
        let index_arg = &arguments[0];
        let mut reachable = self.pending_expression_exit(receiver, falls_through);
        let index_result =
            self.check_pending_operand(index_arg.value, Some(int_type), None, &mut reachable)?;
        if !self.is_error(index_result.ty)
            && !self.is_deferred(index_result.ty)
            && !self.assignable(index_result.ty, int_type)
        {
            self.mismatch(
                self.ast().expressions().get(index_arg.value)?.span(),
                None,
                index_result.ty,
                int_type,
            )?;
        }
        let element_arg = &arguments[1];
        let element_result = self.check_pending_operand(
            element_arg.value,
            Some(element_type),
            None,
            &mut reachable,
        )?;
        if !self.is_error(element_result.ty)
            && !self.is_deferred(element_result.ty)
            && !self.assignable(element_result.ty, element_type)
        {
            self.mismatch(
                self.ast().expressions().get(element_arg.value)?.span(),
                None,
                element_result.ty,
                element_type,
            )?;
        }
        let unit_type = self.builtin(BuiltinType::Unit);
        let function = self.types.intern(TypeKind::Function {
            move_only: false,
            parameters: vec![
                FunctionParameterType {
                    mode: ParameterMode::Value,
                    ty: int_type,
                },
                FunctionParameterType {
                    mode: ParameterMode::Value,
                    ty: element_type,
                },
            ],
            return_type: unit_type,
        });
        self.set_expression(callee, function);
        self.set_expression_category(callee, ExpressionCategory::Temporary);
        self.container_insert_ats
            .push(ContainerInsertAtDescriptor::new(
                expression,
                receiver,
                index_arg.value,
                element_arg.value,
                receiver_ty,
                element_type,
                unit_type,
                call_span,
            ));
        Ok(Some(ExprCheck {
            ty: unit_type,
            falls_through: falls_through
                && index_result.falls_through
                && element_result.falls_through,
        }))
    }
}
