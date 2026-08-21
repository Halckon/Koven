use crate::{
    ast::ExpressionId,
    parser::{FloatLiteralKind, IntegerLiteralKind, LiteralKind},
    source::Span,
};

use super::*;

impl Checker<'_> {
    pub(super) fn check_literal(
        &mut self,
        id: ExpressionId,
        span: Span,
        literal: LiteralKind,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let ty = match literal {
            LiteralKind::Integer(kind) => {
                self.check_integer_literal(id, span, kind, expected, expected_span, false)?
            }
            LiteralKind::Float(kind) => self.check_float_literal(span, kind, expected_span)?,
            LiteralKind::Char => self.builtin(BuiltinType::Char),
            LiteralKind::Boolean(_) => self.builtin(BuiltinType::Boolean),
            LiteralKind::Null => match expected.map(|ty| self.kind(ty).clone()) {
                Some(TypeKind::Nullable(_)) => expected.expect("matched Some"),
                Some(_) => {
                    let nothing = self.builtin(BuiltinType::Nothing);
                    self.types.intern(TypeKind::Nullable(nothing))
                }
                None => {
                    self.emit(
                        self.cannot_infer_code,
                        "cannot infer the type of null without a nullable expected type",
                        span,
                    )?;
                    self.error_type()
                }
            },
        };
        Ok(ExprCheck {
            ty,
            falls_through: true,
        })
    }

    pub(super) fn check_integer_literal(
        &mut self,
        id: ExpressionId,
        span: Span,
        kind: IntegerLiteralKind,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
        negative: bool,
    ) -> Result<TypeId, TypeCheckingError> {
        let Some(magnitude) = self.integer_magnitude(span, kind)? else {
            self.numeric_range_error(span, expected_span)?;
            let error = self.error_type();
            self.set_expression(id, error);
            return Ok(error);
        };
        let expected_builtin = expected.and_then(|ty| match self.kind(ty) {
            TypeKind::Builtin(builtin) => Some(*builtin),
            _ => None,
        });
        let selected = match kind {
            IntegerLiteralKind::Unsuffixed => {
                if let Some(expected) = expected_builtin.filter(|ty| is_signed_integer(*ty)) {
                    fits_signed(magnitude, expected, negative).then_some(expected)
                } else if fits_signed(magnitude, BuiltinType::Int, negative) {
                    Some(BuiltinType::Int)
                } else if fits_signed(magnitude, BuiltinType::Long, negative) {
                    Some(BuiltinType::Long)
                } else {
                    None
                }
            }
            IntegerLiteralKind::Long => {
                fits_signed(magnitude, BuiltinType::Long, negative).then_some(BuiltinType::Long)
            }
            IntegerLiteralKind::Unsigned => {
                if negative {
                    None
                } else if let Some(expected) =
                    expected_builtin.filter(|ty| is_unsigned_integer(*ty))
                {
                    fits_unsigned(magnitude, expected).then_some(expected)
                } else if fits_unsigned(magnitude, BuiltinType::UInt) {
                    Some(BuiltinType::UInt)
                } else if fits_unsigned(magnitude, BuiltinType::ULong) {
                    Some(BuiltinType::ULong)
                } else {
                    None
                }
            }
            IntegerLiteralKind::UnsignedLong => (!negative
                && fits_unsigned(magnitude, BuiltinType::ULong))
            .then_some(BuiltinType::ULong),
        };
        let ty = if let Some(selected) = selected {
            self.builtin(selected)
        } else {
            self.numeric_range_error(span, expected_span)?;
            self.error_type()
        };
        self.set_expression(id, ty);
        Ok(ty)
    }

    fn check_float_literal(
        &mut self,
        span: Span,
        kind: FloatLiteralKind,
        expected_span: Option<Span>,
    ) -> Result<TypeId, TypeCheckingError> {
        let text = self.sources.slice(span)?;
        let number = match kind {
            FloatLiteralKind::Double => text,
            FloatLiteralKind::Float => &text[..text.len() - 1],
        };
        let finite = match kind {
            FloatLiteralKind::Double => number.parse::<f64>().is_ok_and(f64::is_finite),
            FloatLiteralKind::Float => number.parse::<f32>().is_ok_and(f32::is_finite),
        };
        if !finite {
            self.numeric_range_error(span, expected_span)?;
            return Ok(self.error_type());
        }
        Ok(self.builtin(match kind {
            FloatLiteralKind::Double => BuiltinType::Double,
            FloatLiteralKind::Float => BuiltinType::Float,
        }))
    }

    fn integer_magnitude(
        &self,
        span: Span,
        kind: IntegerLiteralKind,
    ) -> Result<Option<u128>, TypeCheckingError> {
        let text = self.sources.slice(span)?;
        let suffix_len = match kind {
            IntegerLiteralKind::Unsuffixed => 0,
            IntegerLiteralKind::Long | IntegerLiteralKind::Unsigned => 1,
            IntegerLiteralKind::UnsignedLong => 2,
        };
        Ok(text[..text.len() - suffix_len].parse::<u128>().ok())
    }

    fn numeric_range_error(
        &mut self,
        span: Span,
        expected_span: Option<Span>,
    ) -> Result<(), TypeCheckingError> {
        if let Some(expected) = expected_span {
            self.emit_with_label(
                self.numeric_range_code,
                "numeric literal is outside the representable range",
                span,
                expected,
                "expected type introduced here",
            )
        } else {
            self.emit(
                self.numeric_range_code,
                "numeric literal is outside the representable range",
                span,
            )
        }
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

fn fits_signed(magnitude: u128, ty: BuiltinType, negative: bool) -> bool {
    let (positive_max, negative_max) = match ty {
        BuiltinType::Byte => (i8::MAX as u128, (i8::MAX as u128) + 1),
        BuiltinType::Short => (i16::MAX as u128, (i16::MAX as u128) + 1),
        BuiltinType::Int => (i32::MAX as u128, (i32::MAX as u128) + 1),
        BuiltinType::Long => (i64::MAX as u128, (i64::MAX as u128) + 1),
        _ => return false,
    };
    magnitude <= if negative { negative_max } else { positive_max }
}

fn fits_unsigned(magnitude: u128, ty: BuiltinType) -> bool {
    magnitude
        <= match ty {
            BuiltinType::UByte => u8::MAX as u128,
            BuiltinType::UShort => u16::MAX as u128,
            BuiltinType::UInt => u32::MAX as u128,
            BuiltinType::ULong => u64::MAX as u128,
            _ => return false,
        }
}
