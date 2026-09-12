//! Compiler-owned values and checked operations, independent of a single-file symbol table.

use std::sync::Arc;

use super::BuiltinType;
use crate::parser::{BinaryOperator as Binary, PrefixOperator as Prefix};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ConstantValue {
    Boolean(bool),
    Integer { ty: BuiltinType, value: i128 },
    Char(char),
    String(Arc<[u8]>),
}

impl ConstantValue {
    pub(super) fn integer(ty: BuiltinType, value: i128) -> Option<Self> {
        let (min, max) = integer_bounds(ty)?;
        (min <= value && value <= max).then_some(Self::Integer { ty, value })
    }

    pub(super) fn prefix(self, operator: Prefix) -> Option<Self> {
        match (operator, self) {
            (Prefix::Not, Self::Boolean(value)) => Some(Self::Boolean(!value)),
            (Prefix::Plus, value @ Self::Integer { .. }) => Some(value),
            (Prefix::Minus, Self::Integer { ty, value }) => Self::integer(ty, -value),
            _ => None,
        }
    }

    pub(super) fn binary(self, operator: Binary, right: Self) -> Option<Self> {
        match (self, right) {
            (
                Self::Integer { ty, value: left },
                Self::Integer {
                    ty: right_ty,
                    value: right,
                },
            ) if ty == right_ty => {
                let value = match operator {
                    Binary::Add => left.checked_add(right)?,
                    Binary::Subtract => left.checked_sub(right)?,
                    Binary::Multiply => left.checked_mul(right)?,
                    Binary::Divide | Binary::Remainder => {
                        // Remainder must reject MIN/-1 too, even though its mathematical result is zero.
                        if right == 0 || (left == integer_bounds(ty)?.0 && right == -1) {
                            return None;
                        }
                        if operator == Binary::Divide {
                            left / right
                        } else {
                            left % right
                        }
                    }
                    Binary::Equal => return Some(Self::Boolean(left == right)),
                    Binary::NotEqual => return Some(Self::Boolean(left != right)),
                    Binary::Less => return Some(Self::Boolean(left < right)),
                    Binary::LessEqual => return Some(Self::Boolean(left <= right)),
                    Binary::Greater => return Some(Self::Boolean(left > right)),
                    Binary::GreaterEqual => return Some(Self::Boolean(left >= right)),
                    _ => return None,
                };
                Self::integer(ty, value)
            }
            (Self::Boolean(left), Self::Boolean(right)) => match operator {
                Binary::LogicalAnd => Some(Self::Boolean(left && right)),
                Binary::LogicalOr => Some(Self::Boolean(left || right)),
                _ => None,
            },
            (Self::String(left), Self::String(right)) => match operator {
                Binary::Add => {
                    let mut bytes = left.to_vec();
                    bytes.extend_from_slice(&right);
                    Some(Self::String(bytes.into()))
                }
                Binary::Equal => Some(Self::Boolean(left == right)),
                Binary::NotEqual => Some(Self::Boolean(left != right)),
                _ => None,
            },
            _ => None,
        }
    }
}

fn integer_bounds(ty: BuiltinType) -> Option<(i128, i128)> {
    Some(match ty {
        BuiltinType::Byte => (i8::MIN.into(), i8::MAX.into()),
        BuiltinType::Short => (i16::MIN.into(), i16::MAX.into()),
        BuiltinType::Int => (i32::MIN.into(), i32::MAX.into()),
        BuiltinType::Long => (i64::MIN.into(), i64::MAX.into()),
        BuiltinType::UByte => (0, u8::MAX.into()),
        BuiltinType::UShort => (0, u16::MAX.into()),
        BuiltinType::UInt => (0, u32::MAX.into()),
        BuiltinType::ULong => (0, u64::MAX.into()),
        _ => return None,
    })
}

/// Decode the lexer-approved escape set without importing a backend or executing user code.
pub(super) fn decode_text(text: &str) -> Option<String> {
    let mut output = String::new();
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        output.push(if character == '\\' {
            match chars.next()? {
                '\\' => '\\',
                '\'' => '\'',
                '"' => '"',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '0' => '\0',
                '$' => '$',
                _ => return None,
            }
        } else {
            character
        });
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_operations_preserve_each_declared_width_and_signedness() {
        for (ty, min, max) in [
            (BuiltinType::Byte, -128, 127),
            (BuiltinType::Short, -32768, 32767),
            (BuiltinType::Int, -2147483648, 2147483647),
            (BuiltinType::Long, -9223372036854775808, 9223372036854775807),
            (BuiltinType::UByte, 0, 255),
            (BuiltinType::UShort, 0, 65535),
            (BuiltinType::UInt, 0, 4294967295),
            (BuiltinType::ULong, 0, 18446744073709551615),
        ] {
            let one = ConstantValue::integer(ty, 1).unwrap();
            let low = ConstantValue::integer(ty, min).unwrap();
            let high = ConstantValue::integer(ty, max).unwrap();
            assert_eq!(
                high.clone().binary(Binary::Add, one.clone()),
                None,
                "{ty:?}"
            );
            assert_eq!(
                low.clone().binary(Binary::Subtract, one.clone()),
                None,
                "{ty:?}"
            );
            assert_eq!(
                high.clone().binary(Binary::Subtract, one),
                ConstantValue::integer(ty, max - 1)
            );
            assert_eq!(
                high.binary(Binary::Multiply, ConstantValue::integer(ty, 2).unwrap()),
                None
            );
            if min < 0 {
                let minus_one = ConstantValue::integer(ty, -1).unwrap();
                assert_eq!(low.clone().binary(Binary::Divide, minus_one.clone()), None);
                assert_eq!(low.binary(Binary::Remainder, minus_one), None);
            }
        }
    }

    #[test]
    fn signed_division_truncates_toward_zero_and_remainder_keeps_dividend_sign() {
        let value = |number| ConstantValue::integer(BuiltinType::Int, number).unwrap();
        for (left, right, quotient, remainder) in [(-7, 3, -2, -1), (7, -3, -2, 1), (-7, -3, 2, -1)]
        {
            assert_eq!(
                value(left).binary(Binary::Divide, value(right)),
                Some(value(quotient))
            );
            assert_eq!(
                value(left).binary(Binary::Remainder, value(right)),
                Some(value(remainder))
            );
        }
    }

    #[test]
    fn strings_preserve_unicode_and_decoded_escape_bytes() {
        assert_eq!(
            decode_text(r#"中\n\r\t\0\$\"\'\\"#).as_deref(),
            Some("中\n\r\t\0$\"'\\")
        );
        let left = ConstantValue::String(Arc::from("中".as_bytes()));
        let right = ConstantValue::String(Arc::from("文".as_bytes()));
        let joined = left.binary(Binary::Add, right).unwrap();
        let expected = ConstantValue::String(Arc::from("中文".as_bytes()));
        assert_eq!(joined.clone(), expected);
        assert_eq!(
            joined.binary(Binary::Equal, expected),
            Some(ConstantValue::Boolean(true))
        );
        assert_eq!(decode_text("\\x"), None);
    }
}
