//! Canonical bindings for host-authorized String extensions, independent of carriers.
use crate::source::{SourceId, Span};

/// Guide-authorized text operations whose standard declarations have been checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextExtensionKind {
    /// 返回 String 的 UTF-8 字节长度。
    ByteLength,
    /// 复制为独立的字节 Array。
    ToBytes,
    /// 按字节边界切片并返回文本 Result。
    SliceBytes,
    /// 长度超限返回 null 的独立 String 连接。
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

    /// 返回宿主授权且与本轮分析绑定的标准源。
    pub const fn source(self) -> SourceId {
        self.source
    }
    /// 返回已验证声明的 canonical callable 身份。
    pub const fn callable(self) -> C {
        self.callable
    }
    /// 返回本轮类型表中的 String receiver 类型。
    pub const fn receiver_type(self) -> T {
        self.receiver_type
    }
    /// 返回 receiver 声明位置，用于追溯和诊断。
    pub const fn receiver_span(self) -> Span {
        self.receiver_span
    }
    /// 返回已通过标准签名检查的操作类别。
    pub const fn kind(self) -> TextExtensionKind {
        self.kind
    }
}
