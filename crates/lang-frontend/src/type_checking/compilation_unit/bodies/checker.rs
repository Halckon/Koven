//! SPEC-0197 compilation-unit body 类型检查 driver。

use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{
        Namespace, SourceUnitId, SourceUnitInput, UnitReferenceTarget,
        ValidatedCompilationUnitNames, ordered_unit_diagnostics,
    },
    parser::{
        CallArgument, Expression, FloatLiteralKind, FunctionBody, FunctionForm, IntegerLiteralKind,
        Item, LiteralKind, ParsedFile, Statement,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitSignatures, ExpressionCategory, TypeCheckingError,
        TypeEnvironment, UnitCallableSignature, UnitTypeId, UnitTypeKind,
        argument_mapping::{MappedParameter, MappingError, map_arguments},
        collect_compilation_unit_signatures,
    },
};

use super::{
    CompilationUnitTypeError, CompilationUnitTypeParts, CompilationUnitTypes,
    UnitCallArgumentDescriptor, UnitCallDescriptor, UnitCallTarget, UnitCallableInstanceKey,
    UnitExpressionId,
};

#[derive(Clone, Copy)]
struct ExpressionCheck {
    ty: UnitTypeId,
    falls_through: bool,
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
    BodyChecker::new(sources, inputs, names, signatures)?.run()
}

struct BodyChecker<'a> {
    sources: &'a SourceMap,
    names: &'a ValidatedCompilationUnitNames,
    files: Vec<&'a ParsedFile>,
    signatures: CompilationUnitSignatures,
    references: BTreeMap<(SourceUnitId, usize, usize, u8), UnitReferenceTarget>,
    parts: CompilationUnitTypeParts,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> BodyChecker<'a> {
    fn new(
        sources: &'a SourceMap,
        inputs: &[SourceUnitInput<'a>],
        names: &'a ValidatedCompilationUnitNames,
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
        Ok(Self {
            sources,
            names,
            files,
            signatures,
            references,
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
                ty: self.literal_type(span, literal, expected, expected_span)?,
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
        self.parts.expression_types.insert(key, result.ty);
        self.parts
            .expression_categories
            .insert(key, self.expression_category(source, expression));
        Ok(result)
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

    fn literal_type(
        &mut self,
        span: Span,
        literal: LiteralKind,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let selected = match literal {
            LiteralKind::Integer(kind) => {
                let text = self.sources.slice(span).map_err(TypeCheckingError::from)?;
                let suffix_len = match kind {
                    IntegerLiteralKind::Unsuffixed => 0,
                    IntegerLiteralKind::Long | IntegerLiteralKind::Unsigned => 1,
                    IntegerLiteralKind::UnsignedLong => 2,
                };
                let magnitude = text[..text.len() - suffix_len].parse::<u128>().ok();
                let selected = match kind {
                    IntegerLiteralKind::Unsuffixed => {
                        let expected = expected
                            .and_then(|ty| self.builtin_kind(ty))
                            .filter(|builtin| is_signed_integer(*builtin));
                        if let Some(expected) = expected {
                            magnitude
                                .filter(|value| fits_integer(*value, expected))
                                .map(|_| expected)
                        } else {
                            magnitude
                                .filter(|value| fits_integer(*value, BuiltinType::Int))
                                .map(|_| BuiltinType::Int)
                                .or_else(|| {
                                    magnitude
                                        .filter(|value| fits_integer(*value, BuiltinType::Long))
                                        .map(|_| BuiltinType::Long)
                                })
                        }
                    }
                    IntegerLiteralKind::Long => magnitude
                        .filter(|value| fits_integer(*value, BuiltinType::Long))
                        .map(|_| BuiltinType::Long),
                    IntegerLiteralKind::Unsigned => {
                        let expected = expected
                            .and_then(|ty| self.builtin_kind(ty))
                            .filter(|builtin| is_unsigned_integer(*builtin));
                        if let Some(expected) = expected {
                            magnitude
                                .filter(|value| fits_integer(*value, expected))
                                .map(|_| expected)
                        } else {
                            magnitude
                                .filter(|value| fits_integer(*value, BuiltinType::UInt))
                                .map(|_| BuiltinType::UInt)
                                .or_else(|| {
                                    magnitude
                                        .filter(|value| fits_integer(*value, BuiltinType::ULong))
                                        .map(|_| BuiltinType::ULong)
                                })
                        }
                    }
                    IntegerLiteralKind::UnsignedLong => magnitude
                        .filter(|value| fits_integer(*value, BuiltinType::ULong))
                        .map(|_| BuiltinType::ULong),
                };
                selected.map(|builtin| self.builtin(builtin))
            }
            LiteralKind::Float(kind) => {
                let text = self.sources.slice(span).map_err(TypeCheckingError::from)?;
                let number = match kind {
                    FloatLiteralKind::Double => text,
                    FloatLiteralKind::Float => &text[..text.len() - 1],
                };
                let finite = match kind {
                    FloatLiteralKind::Double => number.parse::<f64>().is_ok_and(f64::is_finite),
                    FloatLiteralKind::Float => number.parse::<f32>().is_ok_and(f32::is_finite),
                };
                finite.then(|| {
                    self.builtin(match kind {
                        FloatLiteralKind::Double => BuiltinType::Double,
                        FloatLiteralKind::Float => BuiltinType::Float,
                    })
                })
            }
            LiteralKind::Char => Some(self.builtin(BuiltinType::Char)),
            LiteralKind::Boolean(_) => Some(self.builtin(BuiltinType::Boolean)),
            LiteralKind::Null => unreachable!("null is rejected before literal typing"),
        };
        if let Some(ty) = selected {
            return Ok(ty);
        }
        self.emit_maybe_label(
            codes::NUMERIC_LITERAL_OUT_OF_RANGE,
            "numeric literal is outside the representable range",
            span,
            expected_span,
            "expected type introduced here",
        )?;
        Ok(self.error_type())
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

    fn assignable(&self, actual: UnitTypeId, expected: UnitTypeId) -> bool {
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

    fn is_builtin(&self, ty: UnitTypeId, builtin: BuiltinType) -> bool {
        self.signatures.types().get(ty) == Some(&UnitTypeKind::Builtin(builtin))
    }

    fn is_error(&self, ty: UnitTypeId) -> bool {
        matches!(self.signatures.types().get(ty), Some(UnitTypeKind::Error))
    }

    fn builtin(&self, builtin: BuiltinType) -> UnitTypeId {
        self.signatures
            .types()
            .builtin(builtin)
            .expect("UnitTypeTable seeds every builtin")
    }

    fn error_type(&mut self) -> UnitTypeId {
        self.signatures.types_mut().intern(UnitTypeKind::Error)
    }

    fn type_name(&self, ty: UnitTypeId) -> String {
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

    fn emit(
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

    fn file(&self, source: SourceUnitId) -> &'a ParsedFile {
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

fn is_signed_integer(ty: BuiltinType) -> bool {
    matches!(
        ty,
        BuiltinType::Byte | BuiltinType::Short | BuiltinType::Int | BuiltinType::Long
    )
}

fn is_unsigned_integer(ty: BuiltinType) -> bool {
    matches!(
        ty,
        BuiltinType::UByte | BuiltinType::UShort | BuiltinType::UInt | BuiltinType::ULong
    )
}

fn fits_integer(value: u128, ty: BuiltinType) -> bool {
    value
        <= match ty {
            BuiltinType::Byte => i8::MAX as u128,
            BuiltinType::Short => i16::MAX as u128,
            BuiltinType::Int => i32::MAX as u128,
            BuiltinType::Long => i64::MAX as u128,
            BuiltinType::UByte => u8::MAX as u128,
            BuiltinType::UShort => u16::MAX as u128,
            BuiltinType::UInt => u32::MAX as u128,
            BuiltinType::ULong => u64::MAX as u128,
            _ => return false,
        }
}
