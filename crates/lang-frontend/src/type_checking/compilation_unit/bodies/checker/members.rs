//! SPEC-0197 compilation-unit member body、`this` 与 field projection 类型事实。

use std::collections::BTreeMap;

use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{
        DeclarationVisibility, Namespace, SourceUnitId, UnitReferenceTarget, UnitSymbolId,
    },
    parser::{ClassifierBody, ClassifierDeclaration, ClassifierKind, Item, NameMarker},
    source::Span,
    type_checking::{
        BuiltinType, DeferredReason, ParameterMode, TypeCheckingError,
        UnitAggregateProjectionDescriptor, UnitAggregateProjectionKind,
        UnitAggregateProjectionReceiver, UnitCallableSignature, UnitCallableTarget,
        UnitExpressionId, UnitFieldSignature, UnitNominalSignature, UnitTypeId, UnitTypeKind,
    },
};

use super::{BodyChecker, CompilationUnitTypeError, ExpressionCheck, unwrapped_item};

impl BodyChecker<'_> {
    pub(super) fn resolve_bare_symbol_candidates(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        span: Span,
        symbols: &[UnitSymbolId],
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let active = self.flow_facts.get(&super::FlowKey::This).and_then(|ty| {
            match self.signatures.types().get(*ty) {
                Some(UnitTypeKind::EnumCase { case, root }) => Some((*case, *root)),
                _ => None,
            }
        });
        let declarations = self.signatures.declarations().to_vec();
        let mut payloads = Vec::new();
        for declaration in declarations {
            let Some(nominal) = declaration.nominal() else {
                continue;
            };
            for case in nominal.enum_cases() {
                for field in case
                    .payloads()
                    .iter()
                    .filter(|field| symbols.contains(&field.symbol()))
                {
                    payloads.push((nominal.clone(), case.clone(), field.clone()));
                }
            }
        }
        if payloads.is_empty() {
            return Ok(self.deferred_type(DeferredReason::OverloadSelection));
        }
        if let Some((active_case, root)) = active
            && let Some((owner, _, field)) = payloads
                .iter()
                .find(|(_, case, _)| case.type_symbol() == active_case)
                .cloned()
        {
            let arguments = match self.signatures.types().get(root) {
                Some(UnitTypeKind::Nominal { arguments, .. }) => arguments.clone(),
                _ => Vec::new(),
            };
            let substitutions = owner
                .type_parameters()
                .iter()
                .copied()
                .zip(arguments)
                .collect::<BTreeMap<_, _>>();
            let ty = self.substitute_type(field.ty(), &substitutions)?;
            self.parts
                .aggregate_projections
                .push(UnitAggregateProjectionDescriptor::new(
                    UnitExpressionId::new(source, expression),
                    UnitAggregateProjectionReceiver::This(owner.declaration()),
                    field.symbol(),
                    ty,
                    UnitAggregateProjectionKind::Field,
                ));
            return Ok(ty);
        }
        let candidates = payloads
            .iter()
            .map(|(_, case, field)| (field.symbol(), case.name_span()))
            .collect::<Vec<_>>();
        self.emit_payload_access(span, &candidates)?;
        Ok(self.error_type())
    }

    pub(super) fn check_classifier_members(
        &mut self,
        source: SourceUnitId,
        classifier: &ClassifierDeclaration,
        nominal: &UnitNominalSignature,
    ) -> Result<(), CompilationUnitTypeError> {
        let Some(body) = &classifier.body else {
            return Ok(());
        };
        let receiver = if matches!(classifier.kind, ClassifierKind::Interface { .. }) {
            self.signatures
                .types_mut()
                .intern(UnitTypeKind::StaticSelf(nominal.ty()))
        } else {
            nominal.ty()
        };
        let previous = self.current_owner;
        self.current_owner = Some(nominal.declaration());
        let result = self.check_member_body(source, body, nominal, Some(receiver));
        self.current_owner = previous;
        result
    }

    fn check_member_body(
        &mut self,
        source: SourceUnitId,
        body: &ClassifierBody,
        nominal: &UnitNominalSignature,
        receiver: Option<UnitTypeId>,
    ) -> Result<(), CompilationUnitTypeError> {
        for item in &body.members {
            let node = self
                .file(source)
                .ast()
                .items()
                .get(*item)
                .map_err(TypeCheckingError::from)?;
            let span = node.span();
            match unwrapped_item(self.file(source), *item)?.clone() {
                Item::Function { name, form, .. } => {
                    let signature =
                        self.member_signature(source, name, nominal, receiver.is_none())?;
                    let previous = self.current_receiver;
                    let previous_mode = self.current_receiver_mode;
                    self.current_receiver = receiver;
                    self.current_receiver_mode =
                        signature.receiver().map(|receiver| receiver.mode());
                    let result = self.check_function(source, form, &signature);
                    self.current_receiver = previous;
                    self.current_receiver_mode = previous_mode;
                    result?;
                }
                Item::Constant {
                    name,
                    type_ref,
                    initializer,
                    ..
                } => {
                    let NameMarker::Present(name_span) = name else {
                        return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
                    };
                    let symbol = self
                        .symbol_at(source, name_span, Namespace::Value)
                        .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                    let previous = self.current_receiver;
                    self.current_receiver = receiver;
                    let result =
                        self.check_value_initializer(source, symbol, type_ref, initializer);
                    self.current_receiver = previous;
                    result?;
                }
                Item::Companion(companion) if receiver.is_some() => {
                    self.check_member_body(source, &companion.body, nominal, None)?;
                }
                Item::Deinit { body, .. } => {
                    let previous = self.current_receiver;
                    let previous_mode = self.current_receiver_mode;
                    self.current_receiver = receiver;
                    self.current_receiver_mode = Some(ParameterMode::Borrow);
                    let unit_type = self.builtin(BuiltinType::Unit);
                    let previous_return_span = self.current_return_span.take();
                    let previous_flow = std::mem::take(&mut self.flow_facts);
                    self.callable_loop_bases.push(self.loop_depth);
                    let result = self.check_statement(source, body, unit_type, None);
                    self.callable_loop_bases.pop();
                    self.flow_facts = previous_flow;
                    self.current_return_span = previous_return_span;
                    self.current_receiver = previous;
                    self.current_receiver_mode = previous_mode;
                    result?;
                }
                Item::Error => {}
                Item::Modified { .. } => unreachable!("unwrapped_item removes modifiers"),
                Item::Variable { .. } | Item::Classifier(_) | Item::Companion(_) => {
                    return Err(CompilationUnitTypeError::UnsupportedBody(span));
                }
            }
        }
        Ok(())
    }

    fn member_signature(
        &self,
        source: SourceUnitId,
        marker: NameMarker,
        nominal: &UnitNominalSignature,
        companion: bool,
    ) -> Result<UnitCallableSignature, CompilationUnitTypeError> {
        let NameMarker::Present(span) = marker else {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        };
        let symbol = self
            .symbol_at(source, span, Namespace::Value)
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        let signatures = if companion {
            nominal.companion_members()
        } else {
            nominal.members()
        };
        signatures
            .iter()
            .find(|signature| signature.target() == UnitCallableTarget::Symbol(symbol))
            .cloned()
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_member(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        receiver: ExpressionId,
        name_span: Span,
        safe: bool,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if !safe && let Some(ty) = self.check_selected_constant(source, expression, name_span)? {
            return Ok(ExpressionCheck {
                ty,
                falls_through: true,
            });
        }
        if let Some(UnitReferenceTarget::Declaration(declaration)) =
            self.reference(source, name_span, Namespace::Value)
        {
            let signature = self
                .signatures
                .declaration(*declaration)
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            let ty = self
                .parts
                .symbol_types
                .get(&signature.symbol())
                .copied()
                .unwrap_or(signature.ty());
            return Ok(ExpressionCheck {
                ty,
                falls_through: true,
            });
        }
        if let Some(UnitReferenceTarget::Declaration(declaration)) =
            self.reference(source, name_span, Namespace::Type)
        {
            let ty = self
                .signatures
                .declaration(*declaration)
                .map(|signature| signature.ty())
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            return Ok(ExpressionCheck {
                ty,
                falls_through: true,
            });
        }
        let receiver_result = self.check_expression(source, receiver, None, None, return_type)?;
        if let Some(ty) = self.rc_member_type(
            source,
            expression,
            receiver,
            receiver_result.ty,
            name_span,
            safe,
        )? {
            return Ok(ExpressionCheck {
                ty,
                falls_through: receiver_result.falls_through,
            });
        }
        if let Some(ty) = self.container_member_type(
            UnitExpressionId::new(source, expression),
            UnitExpressionId::new(source, receiver),
            receiver_result.ty,
            name_span,
        )? {
            return Ok(ExpressionCheck {
                ty,
                falls_through: receiver_result.falls_through,
            });
        }
        if !safe
            && let Some(ty) = self.check_field_projection(
                source,
                expression,
                receiver,
                receiver_result.ty,
                name_span,
            )?
        {
            return Ok(ExpressionCheck {
                ty,
                falls_through: receiver_result.falls_through,
            });
        }
        if let Some(declaration_span) =
            self.inaccessible_member_span(receiver_result.ty, name_span)?
        {
            self.emit_maybe_label(
                codes::UNRESOLVED_NAME,
                "member is private to its declaring classifier",
                name_span,
                Some(declaration_span),
                "private member declared here",
            )?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through,
            });
        }
        if !safe {
            let candidates = self.payload_candidates(receiver_result.ty, name_span)?;
            if !candidates.is_empty() {
                self.emit_payload_access(name_span, &candidates)?;
                return Ok(ExpressionCheck {
                    ty: self.error_type(),
                    falls_through: receiver_result.falls_through,
                });
            }
        }
        Ok(ExpressionCheck {
            ty: self.deferred_type(DeferredReason::MemberAccess),
            falls_through: receiver_result.falls_through,
        })
    }

    fn check_field_projection(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_type: UnitTypeId,
        name_span: Span,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        let name = self
            .sources
            .slice(name_span)
            .map_err(TypeCheckingError::from)?
            .to_owned();
        let (owner, arguments, field) = match self.signatures.types().get(receiver_type).cloned() {
            Some(UnitTypeKind::EnumCase { case, root }) => {
                let Some((owner, arguments)) = self.nominal_instance(root) else {
                    return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
                };
                let field = owner
                    .enum_cases()
                    .iter()
                    .find(|candidate| candidate.type_symbol() == case)
                    .and_then(|candidate| {
                        candidate
                            .payloads()
                            .iter()
                            .find(|field| field.name() == name)
                    })
                    .cloned();
                (owner, arguments, field)
            }
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => {
                let owner = self
                    .signatures
                    .declaration(declaration)
                    .and_then(|signature| signature.nominal())
                    .cloned()
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                let field = owner
                    .fields()
                    .iter()
                    .find(|field| field.name() == name)
                    .cloned();
                (owner, arguments, field)
            }
            Some(UnitTypeKind::StaticSelf(inner)) => {
                let Some((owner, arguments)) = self.nominal_instance(inner) else {
                    return Ok(None);
                };
                let field = owner
                    .fields()
                    .iter()
                    .find(|field| field.name() == name)
                    .cloned();
                (owner, arguments, field)
            }
            _ => return Ok(None),
        };
        let Some(field) = field else {
            return Ok(None);
        };
        if field.visibility() == DeclarationVisibility::Private
            && self.current_owner != Some(owner.declaration())
        {
            return Ok(None);
        }
        let substitutions = owner
            .type_parameters()
            .iter()
            .copied()
            .zip(arguments)
            .collect::<BTreeMap<_, _>>();
        let ty = self.substitute_type(field.ty(), &substitutions)?;
        self.parts
            .aggregate_projections
            .push(UnitAggregateProjectionDescriptor::new(
                UnitExpressionId::new(source, expression),
                UnitAggregateProjectionReceiver::Expression(UnitExpressionId::new(
                    source, receiver,
                )),
                field.symbol(),
                ty,
                UnitAggregateProjectionKind::Field,
            ));
        Ok(Some(ty))
    }

    fn nominal_instance(&self, ty: UnitTypeId) -> Option<(UnitNominalSignature, Vec<UnitTypeId>)> {
        let UnitTypeKind::Nominal {
            declaration,
            arguments,
        } = self.signatures.types().get(ty)?
        else {
            return None;
        };
        let owner = self
            .signatures
            .declaration(*declaration)?
            .nominal()?
            .clone();
        Some((owner, arguments.clone()))
    }

    fn inaccessible_member_span(
        &self,
        receiver_type: UnitTypeId,
        name_span: Span,
    ) -> Result<Option<Span>, CompilationUnitTypeError> {
        let mut receiver_type = receiver_type;
        while let Some(UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner)) =
            self.signatures.types().get(receiver_type)
        {
            receiver_type = *inner;
        }
        let Some((owner, _)) = self.nominal_instance(receiver_type) else {
            return Ok(None);
        };
        if self.current_owner == Some(owner.declaration()) {
            return Ok(None);
        }
        let name = self
            .sources
            .slice(name_span)
            .map_err(TypeCheckingError::from)?;
        Ok(owner
            .fields()
            .iter()
            .find(|field| {
                field.name() == name && field.visibility() == DeclarationVisibility::Private
            })
            .map(UnitFieldSignature::span)
            .or_else(|| {
                owner
                    .members()
                    .iter()
                    .find(|callable| {
                        callable.name() == name
                            && callable.visibility() == DeclarationVisibility::Private
                    })
                    .map(UnitCallableSignature::name_span)
            }))
    }

    fn payload_candidates(
        &self,
        receiver_type: UnitTypeId,
        name_span: Span,
    ) -> Result<Vec<(UnitSymbolId, Span)>, CompilationUnitTypeError> {
        let mut receiver_type = receiver_type;
        if let Some(UnitTypeKind::Nullable(inner)) = self.signatures.types().get(receiver_type) {
            receiver_type = *inner;
        }
        let root = match self.signatures.types().get(receiver_type) {
            Some(UnitTypeKind::Nominal { declaration, .. }) => Some(*declaration),
            Some(UnitTypeKind::StaticSelf(inner)) => match self.signatures.types().get(*inner) {
                Some(UnitTypeKind::Nominal { declaration, .. }) => Some(*declaration),
                _ => None,
            },
            _ => None,
        };
        let Some(root) = root else {
            return Ok(Vec::new());
        };
        let name = self
            .sources
            .slice(name_span)
            .map_err(TypeCheckingError::from)?;
        Ok(self
            .signatures
            .declaration(root)
            .and_then(|signature| signature.nominal())
            .into_iter()
            .flat_map(|nominal| nominal.enum_cases())
            .flat_map(|case| {
                case.payloads()
                    .iter()
                    .filter(move |field| field.name() == name)
                    .map(move |field| (field.symbol(), case.name_span()))
            })
            .collect())
    }

    fn emit_payload_access(
        &mut self,
        primary: Span,
        candidates: &[(UnitSymbolId, Span)],
    ) -> Result<(), CompilationUnitTypeError> {
        let code = codes::catalog()?.resolve(codes::INVALID_ENUM_PAYLOAD_ACCESS)?;
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            "enum case payload is not uniquely available in the current flow",
            primary,
        )?;
        for (_, case_span) in candidates {
            diagnostic.add_label(self.sources, *case_span, "payload is declared by this case")?;
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }
}

#[cfg(test)]
mod deinit_context_tests {
    use super::BodyChecker;
    use crate::{
        lexer::lex,
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        parser::parse_file,
        source::SourceMap,
        type_checking::{collect_compilation_unit_signatures, standard_environments},
    };

    #[test]
    fn deinit_cannot_target_an_enclosing_checker_loop() {
        for jump in ["break", "continue"] {
            let mut sources = SourceMap::new();
            let source = sources
                .add_source(
                    "deinit.ko",
                    format!("class Resource {{ deinit() {{ {jump} }} }}"),
                )
                .unwrap();
            let lexed = lex(&sources, source).unwrap();
            let file = parse_file(&sources, &lexed).unwrap();
            assert!(file.diagnostics().is_empty());
            let inputs = [SourceUnitInput::new("root", "deinit.ko", source, &file)];
            let (names, types) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &names)
                .unwrap()
                .validate()
                .unwrap();
            let signatures =
                collect_compilation_unit_signatures(&sources, &inputs, &names, &types).unwrap();
            let mut checker =
                BodyChecker::new(&sources, &inputs, &names, &types, signatures).unwrap();
            // Exercise the callable boundary independently of currently unsupported local classifiers.
            checker.loop_depth = 1;
            checker.callable_loop_bases.push(0);
            let typed = checker.run().unwrap();
            assert_eq!(
                typed
                    .diagnostics()
                    .iter()
                    .map(|d| d.code().to_string())
                    .collect::<Vec<_>>(),
                ["L0142"]
            );
        }
    }
}
