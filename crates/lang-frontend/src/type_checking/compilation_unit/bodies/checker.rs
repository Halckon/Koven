//! SPEC-0197 compilation-unit body 类型检查 driver。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{
        DeclarationId, Namespace, SourceUnitId, SourceUnitInput, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames, ordered_unit_diagnostics,
    },
    parser::{Expression, FunctionBody, FunctionForm, Item, ParsedFile, Statement, StringPart},
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitSignatures, DeferredReason, ExpressionUse, ExternalTypeBinding,
        TypeCheckingError, TypeEnvironment, UnitCallableSignature, UnitFunctionParameterType,
        UnitTypeId, UnitTypeKind, collect_compilation_unit_signatures, collect_expression_uses,
    },
};

mod assignment;
mod bindings;
mod calls;
mod construction;
mod container;
mod container_operations;
mod control;
pub(super) mod copyability;
mod destructuring;
mod expression_facts;
mod flow;
mod lambda;
mod literals;
mod members;
mod nullable;
mod operators;
mod postfix;
mod rc;
mod top_level;
mod trial;
mod type_refs;
mod when;

use flow::FlowKey;

use super::{
    CompilationUnitTypeError, CompilationUnitTypeParts, CompilationUnitTypes, UnitExpressionId,
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
    expression_uses: Vec<Vec<ExpressionUse>>,
    signatures: CompilationUnitSignatures,
    references: BTreeMap<(SourceUnitId, usize, usize, u8), UnitReferenceTarget>,
    symbols_by_span: BTreeMap<(SourceUnitId, usize, usize, u8), UnitSymbolId>,
    stable_flow_symbols: BTreeSet<UnitSymbolId>,
    flow_facts: BTreeMap<FlowKey, UnitTypeId>,
    parts: CompilationUnitTypeParts,
    diagnostics: Vec<Diagnostic>,
    current_return_span: Option<Span>,
    loop_depth: usize,
    callable_loop_bases: Vec<usize>,
    candidate_local_expected: bool,
    current_receiver: Option<UnitTypeId>,
    current_owner: Option<DeclarationId>,
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
        let stable_flow_symbols = flow::collect_stable_flow_symbols(&files, names);
        let expression_uses = files
            .iter()
            .map(|file| collect_expression_uses(file))
            .collect();
        Ok(Self {
            sources,
            names,
            environment,
            files,
            expression_uses,
            signatures,
            references,
            symbols_by_span,
            stable_flow_symbols,
            flow_facts: BTreeMap::new(),
            parts: CompilationUnitTypeParts::default(),
            diagnostics: Vec::new(),
            current_return_span: None,
            loop_depth: 0,
            callable_loop_bases: Vec::new(),
            candidate_local_expected: false,
            current_receiver: None,
            current_owner: None,
        })
    }

    fn run(mut self) -> Result<CompilationUnitTypes, CompilationUnitTypeError> {
        let declarations = self.names.names().index().declarations().to_vec();
        for declaration in declarations {
            let signature = self.signatures.declaration(declaration.id()).cloned();
            let callable = signature
                .as_ref()
                .and_then(|signature| signature.callable())
                .cloned();
            let nominal = signature
                .as_ref()
                .and_then(|signature| signature.nominal())
                .cloned();
            let file = self.files[declaration.source_unit().index()];
            let item = unwrapped_item(file, declaration.root())?.clone();
            if matches!(item, Item::Variable { .. } | Item::Constant { .. }) {
                self.check_top_level_initializer(
                    declaration.id(),
                    declaration.source_unit(),
                    &item,
                )?;
                continue;
            }
            if let Some(callable) = callable {
                let Item::Function { form, .. } = item else {
                    return Err(CompilationUnitTypeError::UnsupportedBody(
                        callable.name_span(),
                    ));
                };
                self.check_function(declaration.source_unit(), form, &callable)?;
                continue;
            }
            if let Some(nominal) = nominal {
                let Item::Classifier(classifier) = item else {
                    return Err(CompilationUnitTypeError::UnsupportedBody(
                        file.ast()
                            .items()
                            .get(declaration.root())
                            .map_err(TypeCheckingError::from)?
                            .span(),
                    ));
                };
                self.check_classifier_members(declaration.source_unit(), &classifier, &nominal)?;
                continue;
            }
            if item_requires_body_check(&item) {
                return Err(CompilationUnitTypeError::UnsupportedBody(
                    file.ast()
                        .items()
                        .get(declaration.root())
                        .map_err(TypeCheckingError::from)?
                        .span(),
                ));
            }
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
        self.callable_loop_bases.push(self.loop_depth);
        let result = self.check_function_body(source, form, callable);
        self.callable_loop_bases.pop();
        result
    }

    fn check_function_body(
        &mut self,
        source: SourceUnitId,
        form: FunctionForm,
        callable: &UnitCallableSignature,
    ) -> Result<(), CompilationUnitTypeError> {
        self.flow_facts.clear();
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
        self.current_return_span = expected_span;
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
            Statement::LocalDestructuring {
                bindings,
                left_paren_span,
                right_paren_span,
                initializer,
                ..
            } => self.check_local_destructuring(
                source,
                statement,
                &bindings,
                left_paren_span,
                right_paren_span,
                initializer,
                return_type,
            ),
            Statement::While {
                condition, body, ..
            } => self.check_while_statement(source, condition, body, return_type, return_span),
            Statement::For {
                binding,
                source: iteration_source,
                body,
                ..
            } => self.check_for_statement(
                source,
                &binding,
                iteration_source,
                body,
                return_type,
                return_span,
            ),
            Statement::Loop { body, .. } => {
                self.check_loop_statement(source, body, return_type, return_span)
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
            Expression::Name => {
                match self.check_bare_enum_construction(
                    source,
                    expression,
                    span,
                    expected,
                    expected_span,
                )? {
                    Some(result) => result,
                    None => ExpressionCheck {
                        ty: self.name_type(source, expression, span)?,
                        falls_through: true,
                    },
                }
            }
            Expression::This => {
                let Some(ty) = self
                    .flow_facts
                    .get(&FlowKey::This)
                    .copied()
                    .or(self.current_receiver)
                else {
                    return Err(CompilationUnitTypeError::UnsupportedBody(span));
                };
                ExpressionCheck {
                    ty,
                    falls_through: true,
                }
            }
            Expression::Member {
                receiver,
                name_span,
                safe,
                ..
            } => {
                match self.check_bare_enum_construction(
                    source,
                    expression,
                    name_span,
                    expected,
                    expected_span,
                )? {
                    Some(result) => result,
                    None => self.check_member(
                        source,
                        expression,
                        receiver,
                        name_span,
                        safe,
                        return_type,
                    )?,
                }
            }
            Expression::Literal(literal) => ExpressionCheck {
                ty: self.literal_type(span, literal, expected, expected_span, false)?,
                falls_through: true,
            },
            Expression::String { parts } => {
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        self.check_expression(source, expression, None, None, return_type)?;
                    }
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
            } => match self.check_intrinsic_container_call(
                source,
                expression,
                span,
                callee,
                &type_arguments,
                &arguments,
                expected,
                expected_span,
                return_type,
            )? {
                Some(result) => result,
                None => match self.check_source_construction_call(
                    source,
                    expression,
                    span,
                    callee,
                    &type_arguments,
                    &arguments,
                    expected,
                    expected_span,
                    return_type,
                )? {
                    Some(result) => result,
                    None => self.check_call(
                        source,
                        expression,
                        callee,
                        &type_arguments,
                        &arguments,
                        return_type,
                    )?,
                },
            },
            Expression::Return {
                keyword_span,
                value,
            } => self.check_return(source, keyword_span, value, return_type)?,
            Expression::Break { keyword_span } => self.check_loop_jump(
                keyword_span,
                "break is not inside an enclosing loop in this callable",
            )?,
            Expression::Continue { keyword_span } => self.check_loop_jump(
                keyword_span,
                "continue is not inside an enclosing loop in this callable",
            )?,
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
            } => self.check_binary(
                source,
                operators::BinaryExpression {
                    left,
                    operator,
                    operator_span,
                    right,
                },
                expected,
                expected_span,
                return_type,
            )?,
            Expression::If {
                condition,
                then_branch,
                else_span,
                else_branch,
                ..
            } => self.check_if(
                source,
                condition,
                then_branch,
                else_span,
                else_branch,
                expected,
                expected_span,
                return_type,
            )?,
            Expression::When {
                keyword_span,
                subject,
                entries,
            } => self.check_when(
                source,
                expression,
                keyword_span,
                subject,
                &entries,
                expected,
                expected_span,
                return_type,
            )?,
            Expression::Lambda {
                move_span,
                parameters,
                arrow_span,
                body,
            } => self.check_lambda(
                source,
                span,
                move_span,
                &parameters,
                arrow_span,
                body,
                expected,
                expected_span,
            )?,
            Expression::TypeTest {
                expression,
                operator_span,
                type_ref,
                ..
            } => self.check_type_test(source, expression, operator_span, type_ref, return_type)?,
            Expression::SuperMember { interface, .. } => {
                self.check_super_member(source, interface)?
            }
            Expression::NonNullAssert {
                operand,
                operator_span,
            } => self.check_non_null_assert(source, operand, operator_span, return_type)?,
            Expression::Cast {
                expression,
                type_ref,
                ..
            } => self.check_cast(source, expression, type_ref, return_type)?,
            Expression::Propagate { value, .. } => {
                self.check_propagate(source, value, return_type)?
            }
            Expression::CallableReference { receiver, .. } => {
                self.check_callable_reference(source, receiver, return_type)?
            }
            Expression::Assignment {
                target,
                operator,
                operator_span,
                value,
            } => {
                self.check_assignment(source, target, operator, operator_span, value, return_type)?
            }
            Expression::Index { receiver, index } => {
                self.check_container_index(source, expression, receiver, index, return_type)?
            }
        };
        if let Some(expected) = expected
            && !self.assignable(result.ty, expected)
            && !self.is_error(result.ty)
            && !self.is_error(expected)
            && !self.is_deferred(result.ty)
            && !self.is_deferred(expected)
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
        self.record_nullable_facts(source, expression, result.ty);
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

    fn name_type(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        if let Some(UnitReferenceTarget::Symbol(symbol)) =
            self.reference(source, span, Namespace::Value)
            && let Some(ty) = self.flow_facts.get(&FlowKey::Symbol(*symbol)).copied()
        {
            return Ok(ty);
        }
        match self.reference(source, span, Namespace::Value) {
            Some(UnitReferenceTarget::Declaration(declaration)) => {
                let signature = self
                    .signatures
                    .declaration(*declaration)
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                Ok(self
                    .parts
                    .symbol_types
                    .get(&signature.symbol())
                    .copied()
                    .unwrap_or(signature.ty()))
            }
            Some(UnitReferenceTarget::Symbol(symbol)) => self
                .parts
                .symbol_types
                .get(symbol)
                .copied()
                .or_else(|| self.signatures.symbol_type(*symbol))
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol),
            Some(UnitReferenceTarget::Symbols(symbols)) => {
                let symbols = symbols.clone();
                self.resolve_bare_symbol_candidates(source, expression, span, &symbols)
            }
            Some(UnitReferenceTarget::External(external)) => {
                match self.environment.binding(*external).cloned() {
                    Some(ExternalTypeBinding::Value(ty)) => {
                        Ok(self.normalize_environment_type(&ty))
                    }
                    Some(ExternalTypeBinding::Function(signature)) => {
                        // UnitTypeKind::Function only preserves the callable ABI shape. Until the
                        // unit model can carry compiler-bound identity/effects through a function
                        // value, accepting an effectful external here would silently erase
                        // cross-thread, abort, or stdout facts. Direct calls are handled in calls.rs
                        // and retain those effects.
                        if !signature.effects.is_empty() {
                            return Err(CompilationUnitTypeError::UnsupportedBody(span));
                        }
                        let parameters = signature
                            .parameters
                            .iter()
                            .map(|parameter| {
                                let ty = self.normalize_environment_type(&parameter.ty);
                                UnitFunctionParameterType::new(parameter.mode, ty)
                            })
                            .collect();
                        let return_type = self.normalize_environment_type(&signature.return_type);
                        Ok(self.signatures.types_mut().intern(UnitTypeKind::Function {
                            move_only: false,
                            parameters,
                            return_type,
                        }))
                    }
                    None => Ok(self.deferred_type(DeferredReason::UnboundExternalType)),
                    Some(
                        ExternalTypeBinding::Builtin(_)
                        | ExternalTypeBinding::Capability(_)
                        | ExternalTypeBinding::Intrinsic(_)
                        | ExternalTypeBinding::IntrinsicCallable(_),
                    ) => Err(CompilationUnitTypeError::UnsupportedBody(span)),
                }
            }
            _ => match self.reference(source, span, Namespace::Type) {
                Some(UnitReferenceTarget::Declaration(declaration)) => self
                    .signatures
                    .declaration(*declaration)
                    .map(|signature| signature.ty())
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol),
                _ => Err(CompilationUnitTypeError::UnsupportedBody(span)),
            },
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
            || matches!(
                self.signatures.types().get(actual),
                Some(UnitTypeKind::EnumCase { root, .. }) if *root == expected
            )
            || matches!(
                self.signatures.types().get(actual),
                Some(UnitTypeKind::StaticSelf(interface)) if *interface == expected
            )
            || matches!(
                self.signatures.types().get(expected),
                Some(UnitTypeKind::Nullable(inner))
                    if actual == *inner
                        || matches!(
                            self.signatures.types().get(actual),
                            Some(UnitTypeKind::EnumCase { root, .. }) if root == inner
                        )
            )
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

    pub(super) fn is_deferred(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.signatures.types().get(ty),
            Some(UnitTypeKind::Deferred(_))
        )
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

    pub(super) fn deferred_type(
        &mut self,
        reason: crate::type_checking::DeferredReason,
    ) -> UnitTypeId {
        self.signatures
            .types_mut()
            .intern(UnitTypeKind::Deferred(reason))
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

    pub(super) fn expression_use(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> ExpressionUse {
        self.expression_uses[source.index()][expression.index()]
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
        Item::Classifier(_) => false,
        Item::Modified { .. } => unreachable!("unwrapped_item removes modifiers"),
        Item::Error | Item::Function { .. } | Item::Companion(_) => false,
    }
}
