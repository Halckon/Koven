//! Canonical bindings for host-authorized String extensions, independent of carriers.
use crate::source::{SourceId, Span};

/// Guide-authorized text operations whose standard declarations have been checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextExtensionKind {
    ByteLength,
    ToBytes,
    SliceBytes,
    CheckedPlus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextSignatureType {
    Int,
    String,
    Bytes,
    NullableString,
    TextResult,
    Other,
}

impl TextExtensionKind {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "byteLength" => Some(Self::ByteLength),
            "toBytes" => Some(Self::ToBytes),
            "sliceBytes" => Some(Self::SliceBytes),
            "checkedPlus" => Some(Self::CheckedPlus),
            _ => None,
        }
    }

    pub(crate) fn matches_signature(
        self,
        parameters: &[TextSignatureType],
        result: TextSignatureType,
    ) -> bool {
        use TextSignatureType::*;
        match self {
            Self::ByteLength => parameters.is_empty() && result == Int,
            Self::ToBytes => parameters.is_empty() && result == Bytes,
            Self::SliceBytes => parameters == [Int, Int] && result == TextResult,
            Self::CheckedPlus => parameters == [String] && result == NullableString,
        }
    }
}

/// Same-analysis callable and receiver identities; source spelling alone cannot create one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextExtensionBinding<C, T> {
    source: SourceId,
    callable: C,
    receiver_type: T,
    receiver_span: Span,
    kind: TextExtensionKind,
}

impl<C: Copy, T: Copy> TextExtensionBinding<C, T> {
    pub(crate) fn bind(
        authorized: bool,
        source: SourceId,
        callable: C,
        receiver_type: T,
        receiver_span: Span,
        kind: TextExtensionKind,
    ) -> Option<Self> {
        (authorized && receiver_span.source_id() == source).then_some(Self {
            source,
            callable,
            receiver_type,
            receiver_span,
            kind,
        })
    }

    pub const fn source(self) -> SourceId {
        self.source
    }
    pub const fn callable(self) -> C {
        self.callable
    }
    pub const fn receiver_type(self) -> T {
        self.receiver_type
    }
    pub const fn receiver_span(self) -> Span {
        self.receiver_span
    }
    pub const fn kind(self) -> TextExtensionKind {
        self.kind
    }
}
