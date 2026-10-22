//! A parser-rejected classifier still owns real member bodies, but no nominal identity.

use super::*;
use crate::parser::{ClassifierBody, ClassifierDeclaration, ClassifierKind, NameMarker};

impl BodyChecker<'_> {
    pub(super) fn check_classifier_without_identity(
        &mut self,
        source: SourceUnitId,
        classifier: &ClassifierDeclaration,
    ) -> Result<(), CompilationUnitTypeError> {
        if !self.diagnosed_missing_name(source, classifier.name) {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        }
        for supertype in &classifier.supertypes {
            self.resolve_static_body_type_ref(source, supertype.type_ref)?;
        }
        if let Some(constructor) = &classifier.primary_constructor {
            for field in &constructor.fields {
                let ty = self.resolve_body_type_ref(source, field.type_ref)?;
                self.set_missing_owner_symbol(source, field.name, ty)?;
            }
        }
        let Some(body) = &classifier.body else {
            return Ok(());
        };
        for variant in &body.variants {
            let error = self.error_type();
            for parameter in &variant.parameters {
                self.resolve_body_type_ref(source, parameter.type_ref)?;
                // There is no case identity that could select this payload.
                self.set_missing_owner_symbol(source, parameter.name, error)?;
            }
        }
        // Prepare real member symbols before any body, preserving forward references.
        // A function has no callable target without its owner; Error is not a signature.
        self.prepare_missing_owner_members(source, body)?;
        let kind = match classifier.kind {
            ClassifierKind::ValueClass { .. } => NominalKind::ValueClass,
            ClassifierKind::Class { .. } => NominalKind::Class,
            ClassifierKind::Interface { .. } => NominalKind::Interface,
            ClassifierKind::EnumClass { .. } => NominalKind::EnumClass,
            ClassifierKind::Object { .. } => NominalKind::Object,
        };
        let receiver = self.error_type();
        let previous_owner = self.current_owner.take();
        let result = self.check_missing_owner_body(
            source,
            body,
            Some((receiver, kind)),
            marker_span(classifier.name),
        );
        self.current_owner = previous_owner;
        result
    }

    fn prepare_missing_owner_members(
        &mut self,
        source: SourceUnitId,
        body: &ClassifierBody,
    ) -> Result<(), CompilationUnitTypeError> {
        for &root in &body.members {
            match unwrapped_item(self.file(source), root)?.clone() {
                Item::Function { name, .. } => {
                    let error = self.error_type();
                    self.set_missing_owner_symbol(source, name, error)?;
                }
                Item::Constant { name, type_ref, .. } => {
                    let ty = match type_ref {
                        Some(id) => self.resolve_body_type_ref(source, id)?,
                        None => self.error_type(),
                    };
                    self.set_missing_owner_symbol(source, name, ty)?;
                }
                Item::Companion(companion) => {
                    self.prepare_missing_owner_members(source, &companion.body)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn check_missing_owner_body(
        &mut self,
        source: SourceUnitId,
        body: &ClassifierBody,
        receiver: Option<(UnitTypeId, NominalKind)>,
        owner_span: Span,
    ) -> Result<(), CompilationUnitTypeError> {
        let mut deinit_count = 0;
        for &root in &body.members {
            let item = unwrapped_item(self.file(source), root)?.clone();
            match item {
                Item::Function { name, form, .. } => {
                    if receiver.is_some_and(|(_, kind)| kind != NominalKind::Interface)
                        && matches!(
                            form,
                            FunctionForm::ImplicitUnitAbsent
                                | FunctionForm::Explicit {
                                    body: FunctionBody::Absent,
                                    ..
                                }
                        )
                    {
                        self.emit_maybe_label(
                            codes::CONCRETE_MEMBER_BODY,
                            "concrete classifier member requires a body",
                            marker_span(name),
                            Some(owner_span),
                            "concrete classifier declared here",
                        )?;
                    }
                    self.check_function_without_identity(source, root, receiver)?;
                }
                Item::Constant {
                    name,
                    type_ref,
                    initializer,
                    ..
                } => {
                    let previous_receiver = self.current_receiver;
                    let previous_mode = self.current_receiver_mode;
                    let previous_rejected = self.current_rejected_extension_receiver;
                    self.current_receiver = receiver.map(|(ty, _)| ty);
                    self.current_receiver_mode = None;
                    self.current_rejected_extension_receiver = false;
                    let result =
                        self.check_initializer_without_identity(source, type_ref, initializer);
                    self.current_receiver = previous_receiver;
                    self.current_receiver_mode = previous_mode;
                    self.current_rejected_extension_receiver = previous_rejected;
                    self.set_missing_owner_symbol(source, name, result?)?;
                }
                Item::Companion(companion) if receiver.is_some() => {
                    self.check_missing_owner_body(source, &companion.body, None, owner_span)?;
                }
                Item::Deinit { body, .. } => {
                    deinit_count += 1;
                    if deinit_count > 1 {
                        let span = self
                            .file(source)
                            .ast()
                            .items()
                            .get(root)
                            .map_err(TypeCheckingError::from)?
                            .span();
                        self.emit(
                            codes::DUPLICATE_CALLABLE_SHAPE,
                            "class can declare at most one 'deinit' member",
                            span,
                        )?;
                    }
                    let previous_receiver = self.current_receiver;
                    let previous_mode = self.current_receiver_mode;
                    let previous_rejected = self.current_rejected_extension_receiver;
                    let previous_return_span = self.current_return_span.take();
                    self.current_receiver = receiver.map(|(ty, _)| ty);
                    self.current_receiver_mode = Some(ParameterMode::Borrow);
                    self.current_rejected_extension_receiver = false;
                    self.flow_facts.clear();
                    self.flow_versions.clear();
                    self.callable_loop_bases.push(self.loop_depth);
                    let unit_type = self.builtin(BuiltinType::Unit);
                    let result = self.check_statement(source, body, unit_type, None);
                    self.callable_loop_bases.pop();
                    self.current_receiver = previous_receiver;
                    self.current_receiver_mode = previous_mode;
                    self.current_rejected_extension_receiver = previous_rejected;
                    self.current_return_span = previous_return_span;
                    result?;
                }
                Item::Error => {}
                Item::Modified { .. } => unreachable!("unwrapped_item removes modifiers"),
                Item::Variable { .. } | Item::Classifier(_) | Item::Companion(_) => {
                    let span = self
                        .file(source)
                        .ast()
                        .items()
                        .get(root)
                        .map_err(TypeCheckingError::from)?
                        .span();
                    return Err(CompilationUnitTypeError::UnsupportedBody(span));
                }
            }
        }
        Ok(())
    }

    fn set_missing_owner_symbol(
        &mut self,
        source: SourceUnitId,
        marker: NameMarker,
        ty: UnitTypeId,
    ) -> Result<(), CompilationUnitTypeError> {
        if let NameMarker::Present(span) = marker {
            let symbol = self
                .symbol_at(source, span, Namespace::Value)
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            self.parts.symbol_types.insert(symbol, ty);
        } else if !self.diagnosed_missing_name(source, marker) {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        }
        Ok(())
    }
}

const fn marker_span(marker: NameMarker) -> Span {
    match marker {
        NameMarker::Present(span) | NameMarker::Missing(span) | NameMarker::Error(span) => span,
    }
}
