//! Shared source-literal decoding for type checking, constant evaluation and lowering.

use std::borrow::Cow;

use crate::parser::{FloatLiteralKind, IntegerLiteralKind};

/// 解码整数字面量的非负数学幅值，不决定默认类型、负号或目标类型范围。
///
/// 接受 Lexer 已分类的十进制、十六进制、二进制和数字间下划线，保留 suffix 身份；
/// 非法拼写、后缀不匹配或超过 u128 的幅值返回 None，后续阶段自行提供源码诊断。
pub fn integer_literal_magnitude(text: &str, kind: IntegerLiteralKind) -> Option<u128> {
    let digits = match kind {
        IntegerLiteralKind::Unsuffixed => text,
        IntegerLiteralKind::Long => text.strip_suffix('L')?,
        IntegerLiteralKind::Unsigned => text.strip_suffix(['u', 'U'])?,
        IntegerLiteralKind::UnsignedLong => text.strip_suffix('L')?.strip_suffix(['u', 'U'])?,
    };
    let (digits, radix) = if let Some(digits) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (digits, 16)
    } else if let Some(digits) = digits
        .strip_prefix("0b")
        .or_else(|| digits.strip_prefix("0B"))
    {
        (digits, 2)
    } else {
        (digits, 10)
    };
    let mut value = 0u128;
    let mut previous_digit = false;
    for byte in digits.bytes() {
        if byte == b'_' {
            if !previous_digit {
                return None;
            }
            previous_digit = false;
            continue;
        }
        let digit = char::from(byte).to_digit(radix)?;
        value = value
            .checked_mul(u128::from(radix))?
            .checked_add(u128::from(digit))?;
        previous_digit = true;
    }
    previous_digit.then_some(value)
}

pub(super) fn float_literal_core(text: &str, kind: FloatLiteralKind) -> Option<Cow<'_, str>> {
    let digits = match kind {
        FloatLiteralKind::Double => text,
        FloatLiteralKind::Float => text.strip_suffix(['f', 'F'])?,
    };
    Some(if digits.contains('_') {
        Cow::Owned(digits.replace('_', ""))
    } else {
        Cow::Borrowed(digits)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_decoder_preserves_radix_suffix_and_checked_magnitude() {
        use IntegerLiteralKind::{Long, Unsigned, UnsignedLong, Unsuffixed};
        for (text, kind, expected) in [
            ("0x1f", Unsuffixed, 31),
            ("0XFF", Unsuffixed, 255),
            ("0b10_1010", Unsuffixed, 42),
            ("000_042", Unsuffixed, 42),
            ("0x2AL", Long, 42),
            ("0B101010U", Unsigned, 42),
            ("0xffff_ffff_ffff_ffffuL", UnsignedLong, u64::MAX.into()),
            (
                "340282366920938463463374607431768211455",
                Unsuffixed,
                u128::MAX,
            ),
        ] {
            assert_eq!(
                integer_literal_magnitude(text, kind),
                Some(expected),
                "{text}"
            );
        }
        for text in [
            "",
            "0x",
            "0b2",
            "0x_1",
            "1__0",
            "1_",
            "_1",
            "-1",
            "+1",
            "1u",
            "0x1_0000_0000_0000_0000_0000_0000_0000_0000",
        ] {
            assert_eq!(integer_literal_magnitude(text, Unsuffixed), None, "{text}");
        }
        assert_eq!(integer_literal_magnitude("42", Long), None);
        assert_eq!(integer_literal_magnitude("42ul", UnsignedLong), None);
    }
}
