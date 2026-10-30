//! Storage-only text operations. Public algorithms remain in the trusted Koven source.
use super::{BuiltinType, ExternalTypeBinding, IntrinsicCallable, TypeEnvironment};
use crate::name_resolution::ExternalSymbolId;

/// 封闭的文本存储原语；只有宿主授权的文本源可发布对应调用事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextPrimitiveKind {
    /// 读取已经满足 Int 长度不变量的 UTF-8 字节数。
    ByteLength,
    /// 读取范围内的一个字节，非法索引 Abort。
    ByteAt,
    /// 严格检验字节数组，成功返回 -1，否则返回首错序列的起点。
    Utf8ErrorOffset,
    /// 复制已验证的 UTF-8 字节数组为独立 String。
    CopyBytes,
    /// 复制已验证范围与 scalar 边界的 String 字节区间。
    CopySlice,
}

#[derive(Clone, Copy)]
pub(crate) enum TextOperandType {
    Builtin(BuiltinType),
    Bytes,
}

impl TextPrimitiveKind {
    pub(crate) fn signature(self) -> (&'static [TextOperandType], TextOperandType) {
        use BuiltinType::{Int, String, UByte};
        use TextOperandType::{Builtin as B, Bytes};
        match self {
            Self::ByteLength => (&[B(String)], B(Int)),
            Self::ByteAt => (&[B(String), B(Int)], B(UByte)),
            Self::Utf8ErrorOffset => (&[Bytes], B(Int)),
            Self::CopyBytes => (&[Bytes], B(String)),
            Self::CopySlice => (&[B(String), B(Int), B(Int)], B(String)),
        }
    }
}

impl TypeEnvironment {
    pub(crate) fn text_primitive_kind(
        &self,
        external: ExternalSymbolId,
    ) -> Option<TextPrimitiveKind> {
        match self.binding(external) {
            Some(ExternalTypeBinding::IntrinsicCallable(IntrinsicCallable::Text(kind))) => {
                Some(*kind)
            }
            _ => None,
        }
    }
}
