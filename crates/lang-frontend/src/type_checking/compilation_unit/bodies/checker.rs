//! SPEC-0197 compilation-unit body 类型检查 driver。

use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{
        Namespace, SourceUnitId, SourceUnitInput, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames, ordered_unit_diagnostics,
    },
    parser::{
        CallArgument, Expression, FunctionBody, FunctionForm, Item, LiteralKind, NameMarker,
        ParsedFile, Statement,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitSignatures, ExpressionCategory, TypeCheckingError,
        TypeEnvironment, UnitCallableSignature, UnitTypeId, UnitTypeKind,
        argument_mapping::{MappedParameter, MappingError, map_arguments},
        collect_compilation_unit_signatures,
    },
};

mod literals;
mod operators;
mod type_refs;

use super::{
    CompilationUnitTypeError, CompilationUnitTypeParts, CompilationUnitTypes,
    UnitCallArgumentDescriptor, UnitCallDescriptor, UnitCallTarget, UnitCallableInstanceKey,
    UnitExpressionId,
};

#[derive(Clone, Copy)]
pub(super) struct ExpressionCheck {
    pub(super) ty: UnitTypeId,
    pub(super) falls_through: bool,
}

/// 收集 unit-wide signatures，并在同一个类型空间检查当前已接通的顶层 callable body。
///
/// Signature/body 用户错误始终进入 recovery product；只有输入身份或尚未接通的合法 body
/// 节点才返回内部错误。最终是否可交给 ownership 由 [`CompilationUnitTypes::validate`] 决定。
pub fn check_compilation_unit_types(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
) -> Result<CompilationUnitTypes, CompilationUnitTypeError> {
    let signatures = collect_compilation_unit_signatures(sources, inputs, names, environment)?;
    BodyChecker::new(sources, inputs, names, environment, signatures)?.run()
}

pub(super) struct BodyChecker<'a> {
    sources: &'a SourceMap,
    names: &'a ValidatedCompilationUnitNames,
    environment: &'a TypeEnvironment,
    files: Vec<&'a ParsedFile>,
    signatures: CompilationUnitSignatures,
    references: BTreeMap<(SourceUnitId, usize, usize, u8), UnitReferenceTarget>,
    symbols_by_span: BTreeMap<(SourceUnitId, usize, usize, u8), UnitSymbolId>,
    parts: CompilationUnitTypeParts,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> BodyChecker<'a> {
    fn new(
        sources: &'a SourceMap,
        inputs: &[SourceUnitInput<'a>],
        names: &'a ValidatedCompilationUnitNames,
        environment: &'a TypeEnvironment,
        signatures: CompilationUnitSignatures,
    ) -> Result<Self, CompilationUnitTypeError> {
        let index = names.names().index();
        let mut files = Vec::with_capacity(index.source_units().len());
        for source in index.source_units() {
            let parsed = inputs
                .iter()
                .find(|input| input.source_id() == source.source_id())
                .map(|input| input.parsed())
                .ok_or(CompilationUnitTypeError::MismatchedInputs)?;
            files.push(parsed);
        }
        let references = names
            .names()
            .references()
            .iter()
            .filter_map(|reference| {
                reference.namespace().map(|namespace| {
                    (
                        (
                            reference.source_unit(),
                            reference.span().start(),
                            reference.span().end(),
                            namespace_rank(namespace),
                        ),
                        reference.target().clone(),
                    )
                })
            })
            .collect();
        let symbols_by_span = names
            .names()
            .source_units()
            .iter()
            .flat_map(|unit| {
                unit.resolution().symbols().iter().map(move |symbol| {
                    (
                        (
                            unit.source_unit(),
                            symbol.span().start(),
                            symbol.span().end(),
                            namespace_rank(symbol.namespace()),
                        ),
                        UnitSymbolId::new(unit.source_unit(), symbol.id()),
                    )
                })
            })
            .collect();
        Ok(Self {
            sources,
            names,
            environment,
            files,
            signatures,
            references,
            symbols_by_span,
            parts: CompilationUnitTypeParts::default(),
            diagnostics: Vec::new(),
        })
    }

    fn run(mut self) -> Result<CompilationUnitTypes, CompilationUnitTypeError> {
        let declarations = self.names.names().index().declarations().to_vec();
        for declaration in declarations {
            let callable = self
                .signatures
                .declaration(declaration.id())
                .and_then(|signature| signature.callable())
                .cloned();
            let file = self.files[declaration.source_unit().index()];
            let item = unwrapped_item(file, declaration.root())?;
            let Some(callable) = callable else {
                if item_requires_body_check(item) {
                    return Err(CompilationUnitTypeError::UnsupportedBody(
                        file.ast()
                            .items()
                            .get(declaration.root())
                            .map_err(TypeCheckingError::from)?
                            .span(),
                    ));
                }
                continue;
            };
            let Item::Function { form, .. } = item else {
                return Err(CompilationUnitTypeError::UnsupportedBody(
                    callable.name_span(),
                ));
            };
            self.check_function(declaration.source_unit(), *form, &callable)?;
        }
        let source_units = self.names.names().index().source_units();
        let body_diagnostics =
            ordered_unit_diagnostics(self.sources, source_units, &self.diagnostics)?
                .into_iter()
                .cloned()
                .collect::<Vec<_>>();
        let mut diagnostics = self.signatures.diagnostics().to_vec();
        diagnostics.extend(body_diagnostics.iter().cloned());
        let diagnostics = ordered_unit_diagnostics(self.sources, source_units, &diagnostics)?
            .into_iter()
            .cloned()
            .collect();
        Ok(CompilationUnitTypes::new(
            self.signatures,
            self.parts,
            body_diagnostics,
            diagnostics,
        ))
    }

    fn check_function(
        &mut self,
        source: SourceUnitId,
        form: FunctionForm,
        callable: &UnitCallableSignature,
    ) -> Result<(), CompilationUnitTypeError> {
        let expected_span = match form {
            FunctionForm::Explicit { type_ref, .. } => Some(
                self.file(source)
                    .ast()
                    .type_refs()
                    .get(type_ref)
                    .map_err(TypeCheckingError::from)?
                    .span(),
            ),
            FunctionForm::ImplicitUnitAbsent | FunctionForm::ImplicitUnitBlock(_) => None,
        };
        match form {
            FunctionForm::ImplicitUnitAbsent => Ok(()),
            FunctionForm::ImplicitUnitBlock(body) => {
                self.check_statement(source, body, callable.return_type(), expected_span)?;
                Ok(())
            }
            FunctionForm::Explicit { body, .. } => match body {
                FunctionBody::Absent => Ok(()),
                FunctionBody::Expression { expression, .. } => {
                    self.check_expression(
                        source,
                        expression,
                        Some(callable.return_type()),
                        expected_span,
                        callable.return_type(),
                    )?;
                    Ok(())
                }
                FunctionBody::Block(body) => {
                    let result =
                        self.check_statement(source, body, callable.return_type(), expected_span)?;
                    if result.falls_through
                        && !self.is_builtin(callable.return_type(), BuiltinType::Unit)
                        && !self.is_error(callable.return_type())
                    {
                        let primary = self
                            .file(source)
                            .ast()
                            .statements()
                            .get(body)
                            .map_err(TypeCheckingError::from)?
                            .span();
                        self.emit_maybe_label(
                            codes::MISSING_RETURN,
                            "non-Unit function can reach the end of its body",
                            self.sources
                                .span(primary.source_id(), primary.end(), primary.end())
                                .map_err(TypeCheckingError::from)?,
                            expected_span,
                            "function return type declared here",
                        )?;
                    }
                    Ok(())
                }
            },
        }
    }

    fn check_statement(
        &mut self,
        source: SourceUnitId,
        statement: StatementId,
        return_type: UnitTypeId,
        return_span: Option<Span>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let node = self
            .file(source)
            .ast()
            .statements()
            .get(statement)
            .map_err(TypeCheckingError::from)?;
        let span = node.span();
        let payload = node.payload().clone();
        let unit = self.builtin(BuiltinType::Unit);
        match payload {
            Statement::Block { elements } => {
                let mut falls_through = true;
                for element in elements {
                    let result = self.check_statement(source, element, return_type, return_span)?;
                    falls_through &= result.falls_through;
                }
                Ok(ExpressionCheck {
                    ty: unit,
                    falls_through,
                })
            }
            Statement::Expression { expression } => {
                self.check_expression(source, expression, None, return_span, return_type)
            }
            Statement::LocalVariable { declaration } => {
                self.check_local_variable(source, declaration, return_type)
            }
            Statement::Error => Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            }),
            _ => Err(CompilationUnitTypeError::UnsupportedBody(span)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_expression(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let key = UnitExpressionId::new(source, expression);
        if let Some(ty) = self.parts.expression_types.get(&key).copied() {
            return Ok(ExpressionCheck {
                ty,
                falls_through: !self.is_builtin(ty, BuiltinType::Nothing),
            });
        }
        let node = self
            .file(source)
            .ast()
            .expressions()
            .get(expression)
            .map_err(TypeCheckingError::from)?;
        let span = node.span();
        let payload = node.payload().clone();
        let mut result = match payload {
            Expression::Error => ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            },
            Expression::Name => ExpressionCheck {
                ty: self.name_type(source, span)?,
                falls_through: true,
            },
            Expression::Literal(LiteralKind::Null) => {
                return Err(CompilationUnitTypeError::UnsupportedBody(span));
            }
            Expression::Literal(literal) => ExpressionCheck {
                ty: self.literal_type(span, literal, expected, expected_span, false)?,
                falls_through: true,
            },
            Expression::String { parts } => {
                if parts
                    .iter()
                    .any(|part| matches!(part, crate::parser::StringPart::Interpolation { .. }))
                {
                    return Err(CompilationUnitTypeError::UnsupportedBody(span));
                }
                ExpressionCheck {
                    ty: self.builtin(BuiltinType::String),
                    falls_through: true,
                }
            }
            Expression::Group { expression } => {
                self.check_expression(source, expression, expected, expected_span, return_type)?
            }
            Expression::Call {
                callee,
                type_arguments,
                arguments,
                ..
            } => {
                if !type_arguments.is_empty() {
                    return Err(CompilationUnitTypeError::UnsupportedBody(span));
                }
                self.check_call(source, expression, callee, &arguments, return_type)?
            }
            Expression::Return { value, .. } => {
                if let Some(value) = value {
                    self.check_expression(
                        source,
                        value,
                        Some(return_type),
                        expected_span,
                        return_type,
                    )?;
                } else if !self.is_builtin(return_type, BuiltinType::Unit) {
                    self.emit_maybe_label(
                        codes::RETURN_SHAPE_MISMATCH,
                        "return value does not match the callable return contract",
                        span,
                        expected_span,
                        "function return type declared here",
                    )?;
                }
                ExpressionCheck {
                    ty: self.builtin(BuiltinType::Nothing),
                    falls_through: false,
                }
            }
            Expression::Prefix {
                operator,
                operator_span,
                operand,
            } => self.check_prefix(
                source,
                operator,
                operator_span,
                operand,
                expected,
                expected_span,
                return_type,
            )?,
            Expression::Binary {
                left,
                operator,
                operator_span,
                right,
            } => self.check_binary(source, left, operator, operator_span, right, return_type)?,
            _ => return Err(CompilationUnitTypeError::UnsupportedBody(span)),
        };
        if let Some(expected) = expected
            && !self.assignable(result.ty, expected)
            && !self.is_error(result.ty)
            && !self.is_error(expected)
        {
            self.emit_maybe_label(
                codes::TYPE_MISMATCH,
                "expression type does not match the expected type",
                span,
                expected_span,
                format!(
                    "expected {}, found {}",
                    self.type_name(expected),
                    self.type_name(result.ty)
                ),
            )?;
            result.ty = self.error_type();
        }
        self.record_expression(source, expression, result.ty);
        Ok(result)
    }

    fn check_local_variable(
        &mut self,
        source: SourceUnitId,
        declaration: ItemId,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let Item::Variable {
            name,
            type_ref,
            initializer,
            ..
        } = unwrapped_item(self.file(source), declaration)?.clone()
        else {
            let span = self
                .file(source)
                .ast()
                .items()
                .get(declaration)
                .map_err(TypeCheckingError::from)?
                .span();
            return Err(CompilationUnitTypeError::UnsupportedBody(span));
        };
        let expected = type_ref
            .map(|type_ref| self.resolve_body_type_ref(source, type_ref))
            .transpose()?;
        let expected_span = type_ref
            .map(|type_ref| {
                self.file(source)
                    .ast()
                    .type_refs()
                    .get(type_ref)
                    .map(|node| node.span())
                    .map_err(TypeCheckingError::from)
            })
            .transpose()?;
        let initializer =
            self.check_expression(source, initializer, expected, expected_span, return_type)?;
        self.set_marker_symbol(source, name, expected.unwrap_or(initializer.ty));
        Ok(ExpressionCheck {
            ty: self.builtin(BuiltinType::Unit),
            falls_through: initializer.falls_through,
        })
    }

    fn check_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        callee: ExpressionId,
        arguments: &[CallArgument],
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let callee_span = self
            .file(source)
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?
            .span();
        let call_span = self
            .file(source)
            .ast()
            .expressions()
            .get(expression)
            .map_err(TypeCheckingError::from)?
            .span();
        let target = self
            .reference(source, callee_span, Namespace::Value)
            .cloned();
        let declaration_ids = match target {
            Some(UnitReferenceTarget::Declaration(declaration)) => vec![declaration],
            Some(UnitReferenceTarget::OverloadSet(declarations)) => declarations,
            _ => return Err(CompilationUnitTypeError::UnsupportedBody(callee_span)),
        };
        let mut mapped = Vec::new();
        let mut first_mapping_error = None;
        for declaration in declaration_ids {
            let Some(callable) = self
                .signatures
                .declaration(declaration)
                .and_then(|signature| signature.callable())
                .cloned()
            else {
                continue;
            };
            if !callable.type_parameters().is_empty() {
                return Err(CompilationUnitTypeError::UnsupportedBody(call_span));
            }
            let parameters = callable
                .parameters()
                .iter()
                .map(|parameter| MappedParameter {
                    name: parameter.name().map(str::to_owned),
                    mode: parameter.mode(),
                    ty: parameter.ty(),
                    span: Some(parameter.span()),
                })
                .collect::<Vec<_>>();
            match map_arguments(
                self.sources,
                &parameters,
                arguments,
                call_span,
                |argument| Ok(self.is_syntactic_place(source, argument)),
            )? {
                Ok(mapping) => mapped.push((declaration, callable, mapping)),
                Err(error) => {
                    first_mapping_error.get_or_insert(error);
                }
            }
        }
        if mapped.is_empty() {
            if let Some(error) = first_mapping_error {
                self.emit_mapping_error(error)?;
            } else {
                self.emit(
                    codes::NON_CALLABLE_TARGET,
                    "call target does not have a callable type",
                    callee_span,
                )?;
            }
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        let mut argument_types = Vec::with_capacity(arguments.len());
        for argument in arguments {
            argument_types.push(
                self.check_expression(source, argument.value, None, None, return_type)?
                    .ty,
            );
        }
        let viable = mapped
            .iter()
            .enumerate()
            .filter_map(|(candidate, (_, callable, mapping))| {
                arguments
                    .iter()
                    .enumerate()
                    .all(|(argument_index, _)| {
                        self.assignable(
                            argument_types[argument_index],
                            callable.parameters()[mapping[argument_index]].ty(),
                        )
                    })
                    .then_some(candidate)
            })
            .collect::<Vec<_>>();
        let selected = if viable.len() == 1 {
            viable[0]
        } else {
            self.emit(
                if viable.is_empty() {
                    codes::NO_MATCHING_OVERLOAD
                } else {
                    codes::AMBIGUOUS_CALL
                },
                if viable.is_empty() {
                    "no overload matches the call arguments"
                } else {
                    "call is ambiguous between multiple overloads"
                },
                callee_span,
            )?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        };
        let (declaration, callable, mapping) = mapped.swap_remove(selected);
        let descriptors = mapping
            .iter()
            .enumerate()
            .map(|(argument_index, &parameter_index)| {
                let parameter = &callable.parameters()[parameter_index];
                UnitCallArgumentDescriptor {
                    argument_index,
                    parameter_index,
                    category: self.expression_category(source, arguments[argument_index].value),
                    mode: parameter.mode(),
                    parameter_type: parameter.ty(),
                    cross_thread: false,
                }
            })
            .collect();
        let unit_expression = UnitExpressionId::new(source, expression);
        self.parts.calls.push(UnitCallDescriptor {
            expression: unit_expression,
            instance: UnitCallableInstanceKey {
                target: UnitCallTarget::Declaration(declaration),
                type_arguments: Vec::new(),
            },
            return_type: callable.return_type(),
            arguments: descriptors,
            aborts: false,
            prints_line: false,
        });
        Ok(ExpressionCheck {
            ty: callable.return_type(),
            falls_through: !self.is_builtin(callable.return_type(), BuiltinType::Nothing),
        })
    }

    fn name_type(
        &self,
        source: SourceUnitId,
        span: Span,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        match self.reference(source, span, Namespace::Value) {
            Some(UnitReferenceTarget::Declaration(declaration)) => self
                .signatures
                .declaration(*declaration)
                .map(|signature| signature.ty())
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol),
            Some(UnitReferenceTarget::Symbol(symbol)) => self
                .parts
                .symbol_types
                .get(symbol)
                .copied()
                .or_else(|| self.signatures.symbol_type(*symbol))
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol),
            _ => Err(CompilationUnitTypeError::UnsupportedBody(span)),
        }
    }

    fn expression_category(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> ExpressionCategory {
        if self.is_syntactic_place(source, expression) {
            ExpressionCategory::Place
        } else {
            ExpressionCategory::Temporary
        }
    }

    pub(super) fn record_expression(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        ty: UnitTypeId,
    ) {
        let key = UnitExpressionId::new(source, expression);
        self.parts.expression_types.insert(key, ty);
        self.parts
            .expression_categories
            .insert(key, self.expression_category(source, expression));
    }

    fn set_marker_symbol(&mut self, source: SourceUnitId, marker: NameMarker, ty: UnitTypeId) {
        if let NameMarker::Present(span) = marker
            && let Some(symbol) = self.symbol_at(source, span, Namespace::Value)
        {
            self.parts.symbol_types.insert(symbol, ty);
        }
    }

    fn symbol_at(
        &self,
        source: SourceUnitId,
        span: Span,
        namespace: Namespace,
    ) -> Option<UnitSymbolId> {
        self.symbols_by_span
            .get(&(source, span.start(), span.end(), namespace_rank(namespace)))
            .copied()
    }

    fn is_syntactic_place(&self, source: SourceUnitId, expression: ExpressionId) -> bool {
        let Ok(node) = self.file(source).ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            Expression::Name => matches!(
                self.reference(source, node.span(), Namespace::Value),
                Some(UnitReferenceTarget::Symbol(_))
            ),
            Expression::Group { expression } => self.is_syntactic_place(source, *expression),
            _ => false,
        }
    }

    fn reference(
        &self,
        source: SourceUnitId,
        span: Span,
        namespace: Namespace,
    ) -> Option<&UnitReferenceTarget> {
        self.references
            .get(&(source, span.start(), span.end(), namespace_rank(namespace)))
    }

    pub(super) fn assignable(&self, actual: UnitTypeId, expected: UnitTypeId) -> bool {
        actual == expected
            || self.is_error(actual)
            || self.is_error(expected)
            || self.is_builtin(actual, BuiltinType::Nothing)
    }

    fn builtin_kind(&self, ty: UnitTypeId) -> Option<BuiltinType> {
        match self.signatures.types().get(ty) {
            Some(UnitTypeKind::Builtin(builtin)) => Some(*builtin),
            _ => None,
        }
    }

    pub(super) fn is_builtin(&self, ty: UnitTypeId, builtin: BuiltinType) -> bool {
        self.signatures.types().get(ty) == Some(&UnitTypeKind::Builtin(builtin))
    }

    pub(super) fn is_error(&self, ty: UnitTypeId) -> bool {
        matches!(self.signatures.types().get(ty), Some(UnitTypeKind::Error))
    }

    fn builtin(&self, builtin: BuiltinType) -> UnitTypeId {
        self.signatures
            .types()
            .builtin(builtin)
            .expect("UnitTypeTable seeds every builtin")
    }

    pub(super) fn error_type(&mut self) -> UnitTypeId {
        self.signatures.types_mut().intern(UnitTypeKind::Error)
    }

    pub(super) fn type_name(&self, ty: UnitTypeId) -> String {
        match self.signatures.types().get(ty) {
            Some(UnitTypeKind::Builtin(builtin)) => builtin.name().to_owned(),
            Some(UnitTypeKind::Nominal { declaration, .. }) => {
                format!("declaration#{}", declaration.index())
            }
            Some(UnitTypeKind::Function { .. }) => "function type".to_owned(),
            Some(UnitTypeKind::Error) | None => "<error>".to_owned(),
            Some(kind) => format!("{kind:?}"),
        }
    }

    fn emit_mapping_error(&mut self, error: MappingError) -> Result<(), CompilationUnitTypeError> {
        match error {
            MappingError::Named(primary) => self.emit(
                codes::INVALID_NAMED_ARGUMENT,
                "named argument does not map uniquely to a callable parameter",
                primary,
            ),
            MappingError::Arity(primary) => self.emit(
                codes::CALL_ARGUMENT_ARITY,
                "call must fill every parameter exactly once",
                primary,
            ),
            MappingError::Mode { primary, parameter } => self.emit_maybe_label(
                codes::CALL_ARGUMENT_MODE,
                "argument marker does not match the parameter contract",
                primary,
                parameter,
                "parameter contract declared here",
            ),
        }
    }

    pub(super) fn emit(
        &mut self,
        code: &str,
        message: &str,
        primary: Span,
    ) -> Result<(), CompilationUnitTypeError> {
        let code = codes::catalog()?.resolve(code)?;
        self.diagnostics.push(Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            message,
            primary,
        )?);
        Ok(())
    }

    fn emit_maybe_label(
        &mut self,
        code: &str,
        message: &str,
        primary: Span,
        label: Option<Span>,
        label_message: impl Into<String>,
    ) -> Result<(), CompilationUnitTypeError> {
        let code = codes::catalog()?.resolve(code)?;
        let mut diagnostic =
            Diagnostic::new(self.sources, Severity::Error, code, message, primary)?;
        if let Some(label) = label {
            diagnostic.add_label(self.sources, label, label_message)?;
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    pub(super) fn file(&self, source: SourceUnitId) -> &'a ParsedFile {
        self.files[source.index()]
    }
}

fn namespace_rank(namespace: Namespace) -> u8 {
    match namespace {
        Namespace::Type => 0,
        Namespace::Value => 1,
    }
}

fn unwrapped_item(parsed: &ParsedFile, id: crate::ast::ItemId) -> Result<&Item, TypeCheckingError> {
    let mut item = parsed.ast().items().get(id)?.payload();
    while let Item::Modified { declaration, .. } = item {
        item = parsed.ast().items().get(*declaration)?.payload();
    }
    Ok(item)
}

fn item_requires_body_check(item: &Item) -> bool {
    match item {
        Item::Variable { .. } | Item::Constant { .. } => true,
        Item::Classifier(classifier) => classifier
            .body
            .as_ref()
            .is_some_and(|body| !body.members.is_empty()),
        Item::Modified { .. } => unreachable!("unwrapped_item removes modifiers"),
        Item::Error | Item::Function { .. } | Item::Companion(_) => false,
    }
}
