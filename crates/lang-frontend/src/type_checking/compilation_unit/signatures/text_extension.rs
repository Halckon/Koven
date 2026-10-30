use super::*;
use crate::type_checking::{
    CallableResultSource, IntrinsicTypeConstructor, TextExtensionBinding, TextExtensionKind,
    text_extension::TextSignatureType,
};

impl SignatureCollector<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn bind_text_extension(
        &self,
        source: SourceUnitId,
        target: UnitCallableTarget,
        name: Span,
        generic: bool,
        receiver: UnitCallableReceiver,
        parameters: &[UnitCallableParameter],
        result: UnitTypeId,
        contract: CallableResultSource,
    ) -> Option<TextExtensionBinding<UnitCallableTarget, UnitTypeId>> {
        if generic
            || !matches!(target, UnitCallableTarget::Declaration(_))
            || receiver.mode() != ParameterMode::Borrow
            || self.text_signature_type(receiver.ty()) != TextSignatureType::String
            || contract != CallableResultSource::Owned
            || parameters.iter().any(|p| p.mode() != ParameterMode::Borrow)
        {
            return None;
        }
        let kind = TextExtensionKind::from_name(self.sources.slice(name).ok()?)?;
        let parameters = parameters
            .iter()
            .map(|p| self.text_signature_type(p.ty()))
            .collect::<Vec<_>>();
        if !kind.matches_signature(&parameters, self.text_signature_type(result)) {
            return None;
        }
        let source = self.inputs[source.index()].source_id();
        TextExtensionBinding::bind(
            self.environment.is_authorized_text_source(source),
            source,
            target,
            receiver.ty(),
            receiver.declaration_span(),
            kind,
        )
    }

    fn text_signature_type(&self, ty: UnitTypeId) -> TextSignatureType {
        match self.types.get(ty) {
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) if Some(*declaration) == self.names.builtin_result_declaration()
                && arguments.len() == 2
                && self.types.get(arguments[0])
                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                && self.is_standard_text_error(arguments[1]) =>
            {
                TextSignatureType::TextResult
            }
            Some(UnitTypeKind::Builtin(BuiltinType::String)) => TextSignatureType::String,
            Some(UnitTypeKind::Builtin(BuiltinType::Int)) => TextSignatureType::Int,
            Some(UnitTypeKind::Nullable(inner))
                if self.types.get(*inner) == Some(&UnitTypeKind::Builtin(BuiltinType::String)) =>
            {
                TextSignatureType::NullableString
            }
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Array,
                arguments,
            }) if arguments.len() == 1
                && self.types.get(arguments[0])
                    == Some(&UnitTypeKind::Builtin(BuiltinType::UByte)) =>
            {
                TextSignatureType::Bytes
            }
            _ => TextSignatureType::Other,
        }
    }

    /// Resolve the error by its declaration and host authority, never by a use-site spelling.
    fn is_standard_text_error(&self, ty: UnitTypeId) -> bool {
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.types.get(ty)
        else {
            return false;
        };
        let Some(source) = self.names.index().declarations().get(declaration.index()) else {
            return false;
        };
        arguments.is_empty()
            && source.name() == "TextError"
            && self
                .environment
                .is_authorized_text_source(source.name_span().source_id())
            && self
                .nominals
                .get(declaration)
                .is_some_and(|n| n.kind() == NominalKind::EnumClass)
    }
}
