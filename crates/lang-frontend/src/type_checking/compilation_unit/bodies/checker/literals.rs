//! SPEC-0197 compilation-unit 标量字面量定型。

use crate::{
    diagnostic::codes,
    parser::{FloatLiteralKind, IntegerLiteralKind, LiteralKind},
    source::Span,
    type_checking::{BuiltinType, CompilationUnitTypeError, TypeCheckingError, UnitTypeId},
};

use super::BodyChecker;

impl BodyChecker<'_> {
    pub(super) fn literal_type(
        &mut self,
        span: Span,
        literal: LiteralKind,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        negative: bool,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        if literal == LiteralKind::Null {
            return match expected.and_then(|ty| self.signatures.types().get(ty)) {
                Some(crate::type_checking::UnitTypeKind::Nullable(_)) => {
                    Ok(expected.expect("matched nullable expected type"))
                }
                Some(_) => {
                    let nothing = self.builtin(BuiltinType::Nothing);
                    Ok(self
                        .signatures
                        .types_mut()
                        .intern(crate::type_checking::UnitTypeKind::Nullable(nothing)))
                }
                None => {
                    self.emit(
                        codes::CANNOT_INFER_TYPE,
                        "cannot infer the type of null without a nullable expected type",
                        span,
                    )?;
                    Ok(self.error_type())
                }
            };
        }
        let selected = match literal {
            LiteralKind::Integer(kind) => {
                let text = self.sources.slice(span).map_err(TypeCheckingError::from)?;
                let suffix_len = match kind {
                    IntegerLiteralKind::Unsuffixed => 0,
                    IntegerLiteralKind::Long | IntegerLiteralKind::Unsigned => 1,
                    IntegerLiteralKind::UnsignedLong => 2,
                };
                let magnitude = text[..text.len() - suffix_len].parse::<u128>().ok();
                let selected = match kind {
                    IntegerLiteralKind::Unsuffixed => {
                        let expected = expected
                            .and_then(|ty| self.builtin_kind(ty))
                            .filter(|builtin| is_signed_integer(*builtin));
                        if let Some(expected) = expected {
                            magnitude
                                .filter(|value| fits_signed_integer(*value, expected, negative))
                                .map(|_| expected)
                        } else {
                            magnitude
                                .filter(|value| {
                                    fits_signed_integer(*value, BuiltinType::Int, negative)
                                })
                                .map(|_| BuiltinType::Int)
                                .or_else(|| {
                                    magnitude
                                        .filter(|value| {
                                            fits_signed_integer(*value, BuiltinType::Long, negative)
                                        })
                                        .map(|_| BuiltinType::Long)
                                })
                        }
                    }
                    IntegerLiteralKind::Long => magnitude
                        .filter(|value| fits_signed_integer(*value, BuiltinType::Long, negative))
                        .map(|_| BuiltinType::Long),
                    IntegerLiteralKind::Unsigned => {
                        let expected = expected
                            .and_then(|ty| self.builtin_kind(ty))
                            .filter(|builtin| is_unsigned_integer(*builtin));
                        if negative {
                            None
                        } else if let Some(expected) = expected {
                            magnitude
                                .filter(|value| fits_unsigned_integer(*value, expected))
                                .map(|_| expected)
                        } else {
                            magnitude
                                .filter(|value| fits_unsigned_integer(*value, BuiltinType::UInt))
                                .map(|_| BuiltinType::UInt)
                                .or_else(|| {
                                    magnitude
                                        .filter(|value| {
                                            fits_unsigned_integer(*value, BuiltinType::ULong)
                                        })
                                        .map(|_| BuiltinType::ULong)
                                })
                        }
                    }
                    IntegerLiteralKind::UnsignedLong => (!negative)
                        .then_some(magnitude)
                        .flatten()
                        .filter(|value| fits_unsigned_integer(*value, BuiltinType::ULong))
                        .map(|_| BuiltinType::ULong),
                };
                selected.map(|builtin| self.builtin(builtin))
            }
            LiteralKind::Float(kind) => {
                let text = self.sources.slice(span).map_err(TypeCheckingError::from)?;
                let number = match kind {
                    FloatLiteralKind::Double => text,
                    FloatLiteralKind::Float => &text[..text.len() - 1],
                };
                let finite = match kind {
                    FloatLiteralKind::Double => number.parse::<f64>().is_ok_and(f64::is_finite),
                    FloatLiteralKind::Float => number.parse::<f32>().is_ok_and(f32::is_finite),
                };
                finite.then(|| {
                    self.builtin(match kind {
                        FloatLiteralKind::Double => BuiltinType::Double,
                        FloatLiteralKind::Float => BuiltinType::Float,
                    })
                })
            }
            LiteralKind::Char => Some(self.builtin(BuiltinType::Char)),
            LiteralKind::Boolean(_) => Some(self.builtin(BuiltinType::Boolean)),
            LiteralKind::Null => unreachable!("null is handled before scalar literal typing"),
        };
        if let Some(ty) = selected {
            return Ok(ty);
        }
        self.emit_maybe_label(
            codes::NUMERIC_LITERAL_OUT_OF_RANGE,
            "numeric literal is outside the representable range",
            span,
            expected_span,
            "expected type introduced here",
        )?;
        Ok(self.error_type())
    }
}

fn is_signed_integer(ty: BuiltinType) -> bool {
    matches!(
        ty,
        BuiltinType::Byte | BuiltinType::Short | BuiltinType::Int | BuiltinType::Long
    )
}

fn is_unsigned_integer(ty: BuiltinType) -> bool {
    matches!(
        ty,
        BuiltinType::UByte | BuiltinType::UShort | BuiltinType::UInt | BuiltinType::ULong
    )
}

fn fits_signed_integer(value: u128, ty: BuiltinType, negative: bool) -> bool {
    let (positive_max, negative_max) = match ty {
        BuiltinType::Byte => (i8::MAX as u128, (i8::MAX as u128) + 1),
        BuiltinType::Short => (i16::MAX as u128, (i16::MAX as u128) + 1),
        BuiltinType::Int => (i32::MAX as u128, (i32::MAX as u128) + 1),
        BuiltinType::Long => (i64::MAX as u128, (i64::MAX as u128) + 1),
        _ => return false,
    };
    value <= if negative { negative_max } else { positive_max }
}

fn fits_unsigned_integer(value: u128, ty: BuiltinType) -> bool {
    value
        <= match ty {
            BuiltinType::UByte => u8::MAX as u128,
            BuiltinType::UShort => u16::MAX as u128,
            BuiltinType::UInt => u32::MAX as u128,
            BuiltinType::ULong => u64::MAX as u128,
            _ => return false,
        }
}
