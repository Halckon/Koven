use crate::parser::{
    ClassifierBody, FunctionBody, FunctionForm, Item, NameMarker, ParameterModeMarker, Statement,
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
                    let mut has_error = false;
                    let mut has_deferred = !type_parameters.is_empty();
                    for parameter in parameters {
                        let ty = self.resolve_type_ref(parameter.type_ref)?;
                        has_error |= self.is_error(ty);
                        has_deferred |= self.is_deferred(ty);
                        self.set_marker_symbol(parameter.name, ty);
                        parameter_types.push(FunctionParameterType {
                            mode: item_parameter_mode(parameter.mode_marker),
                            ty,
                        });
                    }
                    let return_type = self.function_return_type(form)?;
                    has_error |= self.is_error(return_type);
                    has_deferred |= self.is_deferred(return_type);
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
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(super) fn check_item(&mut self, id: ItemId) -> Result<(), TypeCheckingError> {
        let payload = self.ast().items().get(id)?.payload().clone();
        match payload {
            Item::Error => {}
            Item::Modified { declaration, .. } => self.check_item(declaration)?,
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
                let expected = type_ref
                    .map(|type_ref| self.resolve_type_ref(type_ref))
                    .transpose()?;
                let expected_span = type_ref
                    .map(|type_ref| self.ast().type_refs().get(type_ref).map(|node| node.span()))
                    .transpose()?;
                let result = self.check_expression(initializer, expected, expected_span)?;
                let ty = expected.unwrap_or(result.ty);
                self.set_marker_symbol(name, ty);
            }
            Item::Function { form, .. } => self.check_function(form)?,
            Item::Classifier(classifier) => {
                if let Some(constructor) = classifier.primary_constructor {
                    for field in constructor.fields {
                        let ty = self.resolve_type_ref(field.type_ref)?;
                        self.set_marker_symbol(field.name, ty);
                    }
                }
                for supertype in classifier.supertypes {
                    self.resolve_type_ref(supertype.type_ref)?;
                }
                if let Some(body) = classifier.body {
                    self.check_classifier_body(body)?;
                }
            }
            Item::Companion(companion) => self.check_classifier_body(companion.body)?,
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

    fn function_return_type(&mut self, form: FunctionForm) -> Result<TypeId, TypeCheckingError> {
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
                initializer,
                ..
            } => {
                self.check_expression(initializer, None, None)?;
                let deferred = self.deferred(DeferredReason::Destructuring);
                for binding in bindings {
                    self.set_marker_symbol(binding, deferred);
                }
                Ok(StatementCheck {
                    ty: unit,
                    falls_through: true,
                })
            }
            Statement::While {
                condition, body, ..
            } => {
                let boolean = self.builtin(BuiltinType::Boolean);
                self.check_expression(condition, Some(boolean), None)?;
                self.check_statement(body)?;
                Ok(StatementCheck {
                    ty: unit,
                    falls_through: true,
                })
            }
            Statement::For { source, body, .. } => {
                self.check_expression(source, None, None)?;
                self.check_statement(body)?;
                Ok(StatementCheck {
                    ty: unit,
                    falls_through: true,
                })
            }
            Statement::Loop { body, .. } => {
                self.check_statement(body)?;
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

    fn set_marker_symbol(&mut self, marker: NameMarker, ty: TypeId) {
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

fn item_parameter_mode(marker: Option<ParameterModeMarker>) -> ParameterMode {
    match marker {
        None => ParameterMode::Value,
        Some(ParameterModeMarker::Borrow(_)) => ParameterMode::Borrow,
        Some(ParameterModeMarker::Inout(_)) => ParameterMode::Inout,
    }
}
