//! Compiler-owned values and checked operations, independent of a single-file symbol table.

use std::sync::Arc;

use super::BuiltinType;
use crate::parser::{BinaryOperator as Binary, PrefixOperator as Prefix};

/// 编译器持有的精确常量值；整数类型保留 width/signedness。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstValue {
    /// Boolean 编译器值。
    Boolean(bool),
    /// 按声明的整数类型规范化的数学值。
    Integer {
        /// 八种整数 builtin 之一，保留精确 width/signedness。
        ty: BuiltinType,
        /// 范围已由 checker 验证，i128 能精确容纳全部八种整数。
        value: i128,
    },
    /// Unicode scalar。
    Char(char),
    /// 编译器持有的 UTF-8 bytes；不表示共享的运行时 String owner。
    String(Arc<[u8]>),
}

impl ConstValue {
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
            let one = ConstValue::integer(ty, 1).unwrap();
            let low = ConstValue::integer(ty, min).unwrap();
            let high = ConstValue::integer(ty, max).unwrap();
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
                ConstValue::integer(ty, max - 1)
            );
            assert_eq!(
                high.binary(Binary::Multiply, ConstValue::integer(ty, 2).unwrap()),
                None
            );
            if min < 0 {
                let minus_one = ConstValue::integer(ty, -1).unwrap();
                assert_eq!(low.clone().binary(Binary::Divide, minus_one.clone()), None);
                assert_eq!(low.binary(Binary::Remainder, minus_one), None);
            }
        }
    }

    #[test]
    fn signed_division_truncates_toward_zero_and_remainder_keeps_dividend_sign() {
        let value = |number| ConstValue::integer(BuiltinType::Int, number).unwrap();
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
        let left = ConstValue::String(Arc::from("中".as_bytes()));
        let right = ConstValue::String(Arc::from("文".as_bytes()));
        let joined = left.binary(Binary::Add, right).unwrap();
        let expected = ConstValue::String(Arc::from("中文".as_bytes()));
        assert_eq!(joined.clone(), expected);
        assert_eq!(
            joined.binary(Binary::Equal, expected),
            Some(ConstValue::Boolean(true))
        );
        assert_eq!(decode_text("\\x"), None);
    }
}

pub(super) fn accepts_prefix_operand(operator: Prefix, ty: BuiltinType) -> bool {
    match operator {
        Prefix::Not => ty == BuiltinType::Boolean,
        Prefix::Plus | Prefix::Minus => integer_bounds(ty).is_some(),
    }
}

pub(super) fn accepts_binary_operand(operator: Binary, ty: BuiltinType) -> bool {
    match operator {
        Binary::LogicalAnd | Binary::LogicalOr => ty == BuiltinType::Boolean,
        Binary::Add | Binary::Equal | Binary::NotEqual => {
            integer_bounds(ty).is_some() || ty == BuiltinType::String
        }
        Binary::Subtract
        | Binary::Multiply
        | Binary::Divide
        | Binary::Remainder
        | Binary::Less
        | Binary::Greater
        | Binary::LessEqual
        | Binary::GreaterEqual => integer_bounds(ty).is_some(),
        _ => false,
    }
}

/// The enabled constant value domain, shared by single-file and compilation-unit gates.
pub(super) fn accepts_constant_type(ty: BuiltinType) -> bool {
    integer_bounds(ty).is_some()
        || matches!(
            ty,
            BuiltinType::Boolean | BuiltinType::Char | BuiltinType::String
        )
}
