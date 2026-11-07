//! SPEC-0197 compilation-unit 标量字面量定型。

use crate::{
    ast::ExpressionId,
    diagnostic::codes,
    name_resolution::SourceUnitId,
    parser::{Expression, FloatLiteralKind, IntegerLiteralKind, LiteralKind, PrefixOperator},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, TypeCheckingError, UnitTypeId, UnitTypeKind,
    },
};

use super::{BodyChecker, ExpressionExpected};

impl BodyChecker<'_> {
    /// A deferred expected type leaves these source integers at their default
    /// widths. Return the concrete widths a later expectation could select.
    pub(super) fn contextual_integer_types(
        &self,
        source: SourceUnitId,
        mut expression: ExpressionId,
    ) -> Result<Vec<UnitTypeId>, CompilationUnitTypeError> {
        let expressions = self.file(source).ast().expressions();
        let get = |id| expressions.get(id).map_err(TypeCheckingError::from);
        let (node, negative) = loop {
            let node = get(expression)?;
            match node.payload() {
                Expression::Group { expression: child } => expression = *child,
                Expression::Prefix {
                    operator: PrefixOperator::Minus,
                    operand,
                    ..
                } => break (get(*operand)?, true),
                _ => break (node, false),
            }
        };
        let Expression::Literal(LiteralKind::Integer(kind)) = node.payload() else {
            return Ok(Vec::new());
        };
        let widths = match kind {
            IntegerLiteralKind::Unsuffixed => [
                BuiltinType::Byte,
                BuiltinType::Short,
                BuiltinType::Int,
                BuiltinType::Long,
            ],
            IntegerLiteralKind::Unsigned if !negative => [
                BuiltinType::UByte,
                BuiltinType::UShort,
                BuiltinType::UInt,
                BuiltinType::ULong,
            ],
            _ => return Ok(Vec::new()),
        };
        let text = self
            .sources
            .slice(node.span())
            .map_err(TypeCheckingError::from)?;
        let Some(magnitude) = crate::type_checking::integer_literal_magnitude(text, *kind) else {
            return Ok(Vec::new());
        };
        Ok(widths
            .into_iter()
            .filter(|&ty| {
                if *kind == IntegerLiteralKind::Unsuffixed {
                    fits_signed_integer(magnitude, ty, negative)
                } else {
                    fits_unsigned_integer(magnitude, ty)
                }
            })
            .map(|ty| self.builtin(ty))
            .collect())
    }

    pub(super) fn null_literal_type(
        &mut self,
        span: Span,
        expected: Option<ExpressionExpected<'_>>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let Some(expected) = expected else {
            self.emit(
                codes::CANNOT_INFER_TYPE,
                "cannot infer the type of null without a nullable expected type",
                span,
            )?;
            return Ok(self.error_type());
        };
        if matches!(expected.kind(self), Some(UnitTypeKind::Nullable(_))) {
            return expected.resolve(self);
        }
        let nothing = self.builtin(BuiltinType::Nothing);
        Ok(self
            .signatures
            .types_mut()
            .intern(UnitTypeKind::Nullable(nothing)))
    }

    pub(super) fn literal_type(
        &mut self,
        span: Span,
        literal: LiteralKind,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        negative: bool,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let selected = match literal {
            LiteralKind::Integer(kind) => {
                let text = self.sources.slice(span).map_err(TypeCheckingError::from)?;
                let magnitude = crate::type_checking::integer_literal_magnitude(text, kind);
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
                let finite = crate::type_checking::literal_value::float_literal_core(text, kind)
                    .is_some_and(|number| match kind {
                        FloatLiteralKind::Double => number.parse::<f64>().is_ok_and(f64::is_finite),
                        FloatLiteralKind::Float => number.parse::<f32>().is_ok_and(f32::is_finite),
                    });
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
