use crate::parser::{
    ClassifierBody, DeclarationModifiers, FunctionBody, FunctionForm, Item, NameMarker, Statement,
};

use super::*;

impl Checker<'_> {
    pub(super) fn predeclare_signatures(&mut self) -> Result<(), TypeCheckingError> {
        let items = self
            .ast()
            .items()
            .iter()
            .map(|(_, node)| node.payload().clone())
            .collect::<Vec<_>>();
        for item in items {
            match item {
                Item::Variable {
                    name,
                    type_ref: Some(type_ref),
                    ..
                }
                | Item::Constant {
                    name,
                    type_ref: Some(type_ref),
                    ..
                } => {
                    let ty = self.resolve_type_ref(type_ref)?;
                    self.set_marker_symbol(name, ty);
                }
                Item::Function {
                    name,
                    type_parameters,
                    parameters,
                    form,
                    ..
                } => {
                    let mut parameter_types = Vec::with_capacity(parameters.len());
                    let mut parameter_symbols = Vec::with_capacity(parameters.len());
                    let mut has_error = false;
                    let mut has_deferred = false;
                    for parameter in parameters {
                        let ty = self.resolve_type_ref(parameter.type_ref)?;
                        let mode = source_parameter_mode(parameter.mode_marker);
                        has_error |= self.is_error(ty);
                        has_deferred |= self.is_deferred(ty);
                        self.set_marker_symbol(parameter.name, ty);
                        let parameter_symbol = match parameter.name {
                            NameMarker::Present(span) => self.symbol_at(span),
                            NameMarker::Missing(_) | NameMarker::Error(_) => None,
                        };
                        if let Some(symbol) = parameter_symbol {
                            self.set_parameter_mode(symbol, mode);
                        }
                        parameter_symbols.push(parameter_symbol);
                        parameter_types.push(FunctionParameterType { mode, ty });
                    }
                    let return_type = self.function_return_type(form)?;
                    has_error |= self.is_error(return_type);
                    has_deferred |= self.is_deferred(return_type);
                    let function_parameters = parameter_types.clone();
                    let function = if has_error {
                        self.error_type()
                    } else if has_deferred {
                        self.deferred(DeferredReason::FunctionContainsDeferred)
                    } else {
                        self.types.intern(TypeKind::Function {
                            move_only: false,
                            parameters: parameter_types,
                            return_type,
                        })
                    };
                    self.set_marker_symbol(name, function);
                    if let NameMarker::Present(span) = name
                        && let Some(symbol) = self.symbol_at(span)
                    {
                        let type_parameters = type_parameters
                            .iter()
                            .filter_map(|parameter| match parameter.name {
                                NameMarker::Present(span) => self.symbol_at(span),
                                _ => None,
                            })
                            .collect();
                        let owner = self
                            .nominal_by_scope
                            .get(&self.symbol_scopes[symbol.index()])
                            .copied();
                        let modifiers = self.function_modifiers(symbol)?;
                        let receiver = owner
                            .and_then(|owner| {
                                self.nominals
                                    .iter()
                                    .find(|descriptor| descriptor.id() == owner)
                                    .cloned()
                            })
                            .and_then(|owner| {
                                let mode = source_parameter_mode(modifiers.receiver_mode);
                                if owner.kind() == NominalKind::Object
                                    && mode != ParameterMode::Borrow
                                {
                                    None
                                } else {
                                    self.symbol_type(owner.id().symbol()).map(|ty| {
                                        CallableReceiverDescriptor {
                                            mode,
                                            ty,
                                            declaration_span: span,
                                            marker_span: modifiers
                                                .receiver_mode
                                                .map(parameter_mode_span),
                                        }
                                    })
                                }
                            });
                        if owner.is_some()
                            && receiver.is_none()
                            && let Some(marker) = modifiers.receiver_mode
                        {
                            self.emit(
                                self.interface_member_mismatch_code,
                                "object instance member receiver must be Borrow",
                                parameter_mode_span(marker),
                            )?;
                        }
                        if let Some(owner) = owner
                            && let Some(descriptor) = self
                                .nominals
                                .iter_mut()
                                .find(|descriptor| descriptor.id() == owner)
                        {
                            descriptor.members.push(symbol);
                        }
                        self.typed_callables.push(CallableDescriptor {
                            symbol,
                            owner,
                            receiver,
                            type_parameters,
                            parameter_symbols,
                            parameters: function_parameters,
                            return_type,
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn function_modifiers(
        &self,
        symbol: SymbolId,
    ) -> Result<DeclarationModifiers, TypeCheckingError> {
        for (_, node) in self.ast().items().iter() {
            let Item::Modified {
                modifiers,
                declaration,
            } = node.payload()
            else {
                continue;
            };
            let Item::Function { name, .. } = self.ast().items().get(*declaration)?.payload()
            else {
                continue;
            };
            if matches!(*name, NameMarker::Present(span) if self.symbol_at(span) == Some(symbol)) {
                return Ok(*modifiers);
            }
        }
        Ok(DeclarationModifiers::default())
    }

    /// const 声明不创建运行时 owner；其类型集合独立于普通变量的可用类型。
    fn check_constant_type(&mut self, ty: TypeId, span: Span) -> Result<(), TypeCheckingError> {
        match self.kind(ty) {
            TypeKind::Error => {}
            // Any 的延后仅涉及运行时表示，其已知类型仍不属于 const 闭合集合。
            TypeKind::Deferred(reason) if *reason != DeferredReason::AnyValueRepresentation => {}
            TypeKind::Builtin(
                BuiltinType::Boolean
                | BuiltinType::Byte
                | BuiltinType::Short
                | BuiltinType::Int
                | BuiltinType::Long
                | BuiltinType::UByte
                | BuiltinType::UShort
                | BuiltinType::UInt
                | BuiltinType::ULong
                | BuiltinType::Char
                | BuiltinType::String,
            ) => {}
            _ => self.emit(
                self.invalid_constant_type_code,
                "constant type must be Boolean, an integer, Char, or String",
                span,
            )?,
        }
        Ok(())
    }

    pub(super) fn check_item(&mut self, id: ItemId) -> Result<(), TypeCheckingError> {
        let payload = self.ast().items().get(id)?.payload().clone();
        let is_constant = matches!(&payload, Item::Constant { .. });
        match payload {
            Item::Error => {}
            Item::Modified {
                modifiers,
                declaration,
            } => {
                let previous = self.current_receiver_mode;
                if !self.classifiers.is_empty()
                    && matches!(
                        self.ast().items().get(declaration)?.payload(),
                        Item::Function { .. }
                    )
                {
                    self.current_receiver_mode =
                        Some(source_parameter_mode(modifiers.receiver_mode));
                }
                let result = self.check_item(declaration);
                self.current_receiver_mode = previous;
                result?;
            }
            Item::Variable {
                name,
                type_ref,
                initializer,
                ..
            }
            | Item::Constant {
                name,
                type_ref,
                initializer,
                ..
            } => {
                if is_constant
                    && let NameMarker::Present(span) = name
                    && let Some(symbol) = self.symbol_at(span)
                {
                    self.constant_items.insert(symbol, id);
                }
                let diagnostics_before = self.diagnostics.len();
                let expected = type_ref
                    .map(|type_ref| self.resolve_type_ref(type_ref))
                    .transpose()?;
                let expected_span = type_ref
                    .map(|type_ref| self.ast().type_refs().get(type_ref).map(|node| node.span()))
                    .transpose()?;
                let result = self.check_expression(initializer, expected, expected_span)?;
                let ty = expected.unwrap_or(result.ty);
                self.set_marker_symbol(name, ty);
                // 常量类型资格不能覆盖或级联已有的普通类型错误。
                if is_constant && self.diagnostics.len() == diagnostics_before {
                    self.check_constant_type(
                        ty,
                        expected_span.unwrap_or(self.ast().expressions().get(initializer)?.span()),
                    )?;
                    if self.diagnostics.len() == diagnostics_before {
                        self.record_constant_dependencies(name, initializer)?;
                    }
                }
            }
            Item::Function { form, .. } => {
                let previous = self.current_receiver_mode;
                if !self.classifiers.is_empty() && self.current_receiver_mode.is_none() {
                    self.current_receiver_mode = Some(ParameterMode::Borrow);
                }
                let result = self.check_function(form);
                self.current_receiver_mode = previous;
                result?;
            }
            Item::Classifier(classifier) => {
                let classifier_type = match classifier.name {
                    NameMarker::Present(span) => self
                        .symbol_at(span)
                        .and_then(|symbol| self.symbol_type(symbol)),
                    NameMarker::Missing(_) | NameMarker::Error(_) => None,
                };
                if let Some(ty) = classifier_type {
                    let this_type = if matches!(
                        classifier.kind,
                        crate::parser::ClassifierKind::Interface { .. }
                    ) {
                        self.types.intern(TypeKind::StaticSelf(ty))
                    } else {
                        ty
                    };
                    self.classifiers.push(this_type);
                }
                for supertype in classifier.supertypes {
                    self.resolve_static_type_ref(supertype.type_ref)?;
                }
                if let Some(body) = classifier.body {
                    self.check_classifier_body(body)?;
                }
                if classifier_type.is_some() {
                    self.classifiers.pop();
                }
            }
            Item::Companion(companion) => {
                let owner = self.classifiers.pop();
                self.check_classifier_body(companion.body)?;
                if let Some(owner) = owner {
                    self.classifiers.push(owner);
                }
            }
        }
        Ok(())
    }

    fn check_classifier_body(&mut self, body: ClassifierBody) -> Result<(), TypeCheckingError> {
        for variant in body.variants {
            for parameter in variant.parameters {
                let ty = self.resolve_type_ref(parameter.type_ref)?;
                self.set_marker_symbol(parameter.name, ty);
            }
        }
        for member in body.members {
            self.check_item(member)?;
        }
        Ok(())
    }

    fn check_function(&mut self, form: FunctionForm) -> Result<(), TypeCheckingError> {
        let return_type = self.function_return_type(form)?;
        let annotation_span = match form {
            FunctionForm::Explicit { type_ref, .. } => {
                Some(self.ast().type_refs().get(type_ref)?.span())
            }
            FunctionForm::ImplicitUnitAbsent | FunctionForm::ImplicitUnitBlock(_) => None,
        };
        self.callables.push(CallableContext {
            return_type,
            annotation_span,
            loop_base: self.loop_depth,
        });
        match form {
            FunctionForm::ImplicitUnitAbsent => {}
            FunctionForm::ImplicitUnitBlock(body) => {
                self.check_statement(body)?;
            }
            FunctionForm::Explicit { body, .. } => match body {
                FunctionBody::Absent => {}
                FunctionBody::Expression { expression, .. } => {
                    let expected = self.known_expected(return_type);
                    self.check_expression(expression, expected, annotation_span)?;
                }
                FunctionBody::Block(body) => {
                    let result = self.check_statement(body)?;
                    if result.falls_through
                        && !self.is_unit(return_type)
                        && !self.is_error(return_type)
                        && !self.is_deferred(return_type)
                    {
                        let primary = self.body_end_span(body)?;
                        if let Some(label) = annotation_span {
                            self.emit_with_label(
                                self.missing_return_code,
                                "non-Unit function can reach the end of its body",
                                primary,
                                label,
                                format!("function returns {}", self.type_name(return_type)),
                            )?;
                        } else {
                            self.emit(
                                self.missing_return_code,
                                "non-Unit function can reach the end of its body",
                                primary,
                            )?;
                        }
                    }
                }
            },
        }
        self.callables.pop();
        Ok(())
    }

    pub(super) fn function_return_type(
        &mut self,
        form: FunctionForm,
    ) -> Result<TypeId, TypeCheckingError> {
        match form {
            FunctionForm::ImplicitUnitAbsent | FunctionForm::ImplicitUnitBlock(_) => {
                Ok(self.builtin(BuiltinType::Unit))
            }
            FunctionForm::Explicit { type_ref, .. } => self.resolve_type_ref(type_ref),
        }
    }

    pub(super) fn check_statement(
        &mut self,
        id: StatementId,
    ) -> Result<StatementCheck, TypeCheckingError> {
        let payload = self.ast().statements().get(id)?.payload().clone();
        let unit = self.builtin(BuiltinType::Unit);
        match payload {
            Statement::Error => Ok(StatementCheck {
                ty: self.error_type(),
                falls_through: true,
            }),
            Statement::Block { elements } => {
                let falls_through = self.check_elements(&elements)?;
                Ok(StatementCheck {
                    ty: unit,
                    falls_through,
                })
            }
            Statement::LambdaBody { elements } | Statement::ControlBody { elements } => {
                self.check_value_elements(&elements, None, None)
            }
            Statement::LocalVariable { declaration } => {
                self.check_item(declaration)?;
                Ok(StatementCheck {
                    ty: unit,
                    falls_through: true,
                })
            }
            Statement::LocalDestructuring {
                bindings,
                left_paren_span,
                right_paren_span,
                initializer,
                ..
            } => self.check_local_destructuring(
                id,
                &bindings,
                left_paren_span,
                right_paren_span,
                initializer,
            ),
            Statement::While {
                condition, body, ..
            } => {
                let boolean = self.builtin(BuiltinType::Boolean);
                self.check_expression(condition, Some(boolean), None)?;
                self.check_loop_body(body)?;
                Ok(StatementCheck {
                    ty: unit,
                    falls_through: true,
                })
            }
            Statement::For { source, body, .. } => {
                self.check_expression(source, None, None)?;
                self.check_loop_body(body)?;
                Ok(StatementCheck {
                    ty: unit,
                    falls_through: true,
                })
            }
            Statement::Loop { body, .. } => {
                self.check_loop_body(body)?;
                Ok(StatementCheck {
                    ty: unit,
                    falls_through: true,
                })
            }
            Statement::Expression { expression } => {
                let result = self.check_expression(expression, None, None)?;
                Ok(StatementCheck {
                    ty: result.ty,
                    falls_through: result.falls_through,
                })
            }
        }
    }

    fn check_loop_body(&mut self, body: StatementId) -> Result<StatementCheck, TypeCheckingError> {
        self.loop_depth += 1;
        let result = self.check_statement(body);
        self.loop_depth -= 1;
        result
    }

    pub(super) fn check_value_body(
        &mut self,
        id: StatementId,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<StatementCheck, TypeCheckingError> {
        let payload = self.ast().statements().get(id)?.payload().clone();
        match payload {
            Statement::LambdaBody { elements } | Statement::ControlBody { elements } => {
                self.check_value_elements(&elements, expected, expected_span)
            }
            Statement::Expression { expression } => {
                let result = self.check_expression(expression, expected, expected_span)?;
                Ok(StatementCheck {
                    ty: result.ty,
                    falls_through: result.falls_through,
                })
            }
            _ => self.check_statement(id),
        }
    }

    fn check_elements(&mut self, elements: &[StatementId]) -> Result<bool, TypeCheckingError> {
        let mut falls_through = true;
        for &element in elements {
            let result = self.check_statement(element)?;
            falls_through &= result.falls_through;
        }
        Ok(falls_through)
    }

    fn check_value_elements(
        &mut self,
        elements: &[StatementId],
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<StatementCheck, TypeCheckingError> {
        let unit = self.builtin(BuiltinType::Unit);
        let Some((&last, prefix)) = elements.split_last() else {
            if let Some(expected) = expected
                && !self.assignable(unit, expected)
            {
                let span = expected_span.expect("known expected body type has a source span");
                self.mismatch(span, expected_span, unit, expected)?;
            }
            return Ok(StatementCheck {
                ty: unit,
                falls_through: true,
            });
        };
        let mut falls_through = self.check_elements(prefix)?;
        let last_payload = self.ast().statements().get(last)?.payload().clone();
        let result = if let Statement::Expression { expression } = last_payload {
            let expression = self.check_expression(expression, expected, expected_span)?;
            StatementCheck {
                ty: expression.ty,
                falls_through: expression.falls_through,
            }
        } else {
            self.check_statement(last)?
        };
        falls_through &= result.falls_through;
        Ok(StatementCheck {
            ty: result.ty,
            falls_through,
        })
    }

    pub(super) fn set_marker_symbol(&mut self, marker: NameMarker, ty: TypeId) {
        if let NameMarker::Present(span) = marker
            && let Some(symbol) = self.symbol_at(span)
        {
            self.set_symbol(symbol, ty);
        }
    }

    fn known_expected(&self, ty: TypeId) -> Option<TypeId> {
        (!self.is_error(ty) && !self.is_deferred(ty)).then_some(ty)
    }

    pub(super) fn is_unit(&self, ty: TypeId) -> bool {
        matches!(self.kind(ty), TypeKind::Builtin(BuiltinType::Unit))
    }

    fn body_end_span(&self, body: StatementId) -> Result<Span, TypeCheckingError> {
        let span = self.ast().statements().get(body)?.span();
        let text = self.sources.slice(span)?;
        if text.ends_with('}') {
            Ok(self
                .sources
                .span(span.source_id(), span.end() - 1, span.end())?)
        } else {
            Ok(self
                .sources
                .span(span.source_id(), span.end(), span.end())?)
        }
    }
}
