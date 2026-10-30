use super::*;
use crate::type_checking::{
    CallableResultSource, CallableTarget, TextExtensionBinding, TextExtensionKind,
    text_extension::TextSignatureType,
};

impl Checker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn bind_text_extension(
        &self,
        symbol: SymbolId,
        name: Span,
        generic: bool,
        receiver: CallableReceiverDescriptor,
        parameters: &[FunctionParameterType],
        result: TypeId,
        contract: CallableResultSource,
    ) -> Option<TextExtensionBinding<CallableTarget, TypeId>> {
        if generic
            || receiver.mode() != ParameterMode::Borrow
            || self.text_signature_type(receiver.ty()) != TextSignatureType::String
            || contract != CallableResultSource::Owned
            || parameters.iter().any(|p| p.mode != ParameterMode::Borrow)
        {
            return None;
        }
        let kind = TextExtensionKind::from_name(self.sources.slice(name).ok()?)?;
        let parameters = parameters
            .iter()
            .map(|p| self.text_signature_type(p.ty))
            .collect::<Vec<_>>();
        if !kind.matches_signature(&parameters, self.text_signature_type(result)) {
            return None;
        }
        TextExtensionBinding::bind(
            self.environment
                .is_authorized_text_source(self.parsed.source_id()),
            self.parsed.source_id(),
            CallableTarget::Source(symbol),
            receiver.ty(),
            receiver.declaration_span(),
            kind,
        )
    }

    fn text_signature_type(&self, ty: TypeId) -> TextSignatureType {
        match self.types.get(ty) {
            Some(TypeKind::Builtin(BuiltinType::String)) => TextSignatureType::String,
            Some(TypeKind::Builtin(BuiltinType::Int)) => TextSignatureType::Int,
            Some(TypeKind::Nullable(inner))
                if self.types.get(*inner) == Some(&TypeKind::Builtin(BuiltinType::String)) =>
            {
                TextSignatureType::NullableString
            }
            Some(TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Array,
                arguments,
            }) if arguments.len() == 1
                && self.types.get(arguments[0]) == Some(&TypeKind::Builtin(BuiltinType::UByte)) =>
            {
                TextSignatureType::Bytes
            }
            _ => TextSignatureType::Other,
        }
    }
}
