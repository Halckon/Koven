//! Check real parser-recovered declarations without inventing declaration/callable identities.

use super::*;
use crate::{
    ast::TypeRefId,
    parser::{DeclarationModifiers, NameMarker, ParameterModeMarker},
    type_checking::{
        BorrowReturnOrigin, CallableResultSource, IntrinsicTypeConstructor,
        compilation_unit::signatures::diagnosed_missing_name,
    },
};

impl BodyChecker<'_> {
    pub(super) fn diagnosed_missing_name(&self, source: SourceUnitId, name: NameMarker) -> bool {
        diagnosed_missing_name(self.file(source), name)
    }

    pub(super) fn check_missing_name_roots(&mut self) -> Result<(), CompilationUnitTypeError> {
        let sources = self.names.index().source_units().to_vec();
        for source in sources {
            let roots = self.file(source.id()).roots().to_vec();
            for root in roots {
                let item = unwrapped_item(self.file(source.id()), root)?.clone();
                let name = match &item {
                    Item::Function { name, .. }
                    | Item::Variable { name, .. }
                    | Item::Constant { name, .. } => *name,
                    Item::Classifier(classifier) => classifier.name,
                    _ => continue,
                };
                if matches!(name, NameMarker::Present(_)) {
                    continue;
                }
                if !self.diagnosed_missing_name(source.id(), name) {
                    return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
                }
                match item {
                    Item::Function { .. } => {
                        self.check_function_without_identity(source.id(), root, None)?;
                    }
                    Item::Variable {
                        type_ref,
                        initializer,
                        ..
                    }
                    | Item::Constant {
                        type_ref,
                        initializer,
                        ..
                    } => {
                        self.check_initializer_without_identity(
                            source.id(),
                            type_ref,
                            initializer,
                        )?;
                    }
                    Item::Classifier(classifier) => {
                        self.check_classifier_without_identity(source.id(), &classifier)?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_initializer_without_identity(
        &mut self,
        source: SourceUnitId,
        type_ref: Option<TypeRefId>,
        initializer: ExpressionId,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let expected = type_ref
            .map(|id| self.resolve_body_type_ref(source, id))
            .transpose()?;
        let expected_span = type_ref
            .map(|id| {
                self.file(source)
                    .ast()
                    .type_refs()
                    .get(id)
                    .map(|node| node.span())
            })
            .transpose()
            .map_err(TypeCheckingError::from)?;
        self.flow_facts.clear();
        self.flow_versions.clear();
        let previous_return_span = self.current_return_span.take();
        let return_type = self.error_type();
        let result =
            self.check_expression(source, initializer, expected, expected_span, return_type);
        self.current_return_span = previous_return_span;
        result.map(|result| expected.unwrap_or(result.ty))
    }

    pub(super) fn check_function_without_identity(
        &mut self,
        source: SourceUnitId,
        root: ItemId,
        receiver_owner: Option<(UnitTypeId, NominalKind)>,
    ) -> Result<(), CompilationUnitTypeError> {
        let modifiers = match self
            .file(source)
            .ast()
            .items()
            .get(root)
            .map_err(TypeCheckingError::from)?
            .payload()
        {
            Item::Modified { modifiers, .. } => *modifiers,
            _ => DeclarationModifiers::default(),
        };
        let item = unwrapped_item(self.file(source), root)?.clone();
        let Item::Function {
            parameters,
            form,
            extension_receiver,
            ..
        } = &item
        else {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        };
        if extension_receiver.is_none()
            && let Some((span, message)) =
                crate::type_checking::declaration_frontier::unsupported_declaration(
                    &item,
                    self.environment
                        .is_authorized_range_source(self.file(source).source_id()),
                )
        {
            self.emit(codes::UNSUPPORTED_BORROW_FLOW, message, span)?;
        }
        let mut receiver = receiver_owner.map(|(ty, _)| ty);
        let mut contract_receiver = receiver;
        let mut receiver_mode = receiver_owner.map(|_| parameter_mode(modifiers.receiver_mode));
        if let Some(extension) = extension_receiver {
            contract_receiver = Some(self.resolve_body_type_ref(source, extension.type_ref)?);
            self.emit(
                codes::UNSUPPORTED_BORROW_FLOW,
                "extension receiver lacks a trusted canonical Borrow List/View binding",
                extension.dot_span,
            )?;
            receiver = Some(self.error_type());
        }
        if let Some((_, NominalKind::Object)) = receiver_owner
            && let Some(marker) = modifiers.receiver_mode
            && parameter_mode(Some(marker)) != ParameterMode::Borrow
        {
            self.emit(
                codes::INTERFACE_MEMBER_MISMATCH,
                "object instance member receiver must be Borrow",
                mode_span(marker),
            )?;
            receiver = Some(self.error_type());
            receiver_mode = None;
        }
        let mut parameter_types = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            let ty = self.resolve_body_type_ref(source, parameter.type_ref)?;
            if let NameMarker::Present(span) = parameter.name {
                self.symbol_at(source, span, Namespace::Value)
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                self.set_span_symbol(source, span, ty);
                self.set_parameter_mode(source, span, parameter_mode(parameter.mode_marker));
            } else if !self.diagnosed_missing_name(source, parameter.name) {
                return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
            }
            parameter_types.push(ty);
        }
        let return_type = match form {
            FunctionForm::ImplicitUnitAbsent | FunctionForm::ImplicitUnitBlock(_) => {
                self.builtin(BuiltinType::Unit)
            }
            FunctionForm::Explicit { type_ref, .. } => {
                self.resolve_body_type_ref(source, *type_ref)?
            }
        };
        let contract = match crate::type_checking::result_source::resolve_result_source(
            self.sources,
            *form,
            parameters,
            extension_receiver
                .map(|_| parameter_mode(modifiers.receiver_mode))
                .or(receiver_mode),
        )
        .map_err(TypeCheckingError::from)?
        {
            Ok(contract) => contract,
            Err(issue) => {
                self.emit(codes::INVALID_BORROW_CONTRACT, issue.message, issue.span)?;
                CallableResultSource::Owned
            }
        };
        if let CallableResultSource::Carrier(carrier) = contract {
            let source_type = match carrier.origin() {
                BorrowReturnOrigin::Parameter(index) => parameter_types.get(index).copied(),
                BorrowReturnOrigin::Receiver => contract_receiver,
            };
            let element = |ty| match self.signatures.types().get(ty) {
                Some(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::View | IntrinsicTypeConstructor::List,
                    arguments,
                }) => arguments.first().copied(),
                _ => None,
            };
            let result_element = match self.signatures.types().get(return_type) {
                Some(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::View,
                    arguments,
                }) => arguments.first().copied(),
                _ => None,
            };
            if !self.is_error(return_type)
                && !source_type.is_some_and(|ty| self.is_error(ty))
                && let Some(issue) = crate::type_checking::result_source::carrier_shape_issue(
                    contract,
                    result_element,
                    source_type.and_then(element),
                )
            {
                self.emit(codes::INVALID_BORROW_CONTRACT, issue.message, issue.span)?;
            }
        }
        let previous_receiver = std::mem::replace(&mut self.current_receiver, receiver);
        let previous_mode = std::mem::replace(&mut self.current_receiver_mode, receiver_mode);
        let previous_rejected =
            std::mem::replace(&mut self.current_rejected_extension_receiver, false);
        let previous_return_span = self.current_return_span;
        self.callable_loop_bases.push(self.loop_depth);
        let result = self.check_function_body(source, *form, return_type);
        self.callable_loop_bases.pop();
        self.current_receiver = previous_receiver;
        self.current_receiver_mode = previous_mode;
        self.current_rejected_extension_receiver = previous_rejected;
        self.current_return_span = previous_return_span;
        result
    }
}

const fn parameter_mode(marker: Option<ParameterModeMarker>) -> ParameterMode {
    match marker {
        None | Some(ParameterModeMarker::Borrow(_)) => ParameterMode::Borrow,
        Some(ParameterModeMarker::Own(_)) => ParameterMode::Value,
        Some(ParameterModeMarker::Inout(_)) => ParameterMode::Inout,
    }
}

const fn mode_span(marker: ParameterModeMarker) -> Span {
    match marker {
        ParameterModeMarker::Borrow(span)
        | ParameterModeMarker::Own(span)
        | ParameterModeMarker::Inout(span) => span,
    }
}
