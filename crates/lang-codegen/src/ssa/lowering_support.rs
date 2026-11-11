//! 单文件与 compilation-unit adapter 共用的中立 lowering 支撑。

pub(in crate::ssa) mod callable_instances;

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
    parser::{Expression, IntegerLiteralKind, LiteralKind, NameMarker, ParsedFile},
    source::Span,
    type_checking::{CompilationUnitTypes, UnitCallReceiverOrigin, UnitExpressionId},
};

pub(in crate::ssa) mod string_literal {
    //! 编译器封闭 effect 使用的静态 String literal 解码。

    use lang_frontend::{
        ast::ExpressionId,
        parser::{Expression, ParsedFile, StringPart},
        source::Span,
    };

    use super::{LoweringError, LoweringErrorKind, error};

    pub(in crate::ssa) fn decode_plain(
        parsed: &ParsedFile,
        source_text: &str,
        expression: ExpressionId,
    ) -> Result<Option<Vec<u8>>, LoweringError> {
        let node =
            parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?;
        match node.payload() {
            Expression::Group { expression } => decode_plain(parsed, source_text, *expression),
            Expression::String { parts } => {
                let mut bytes = Vec::new();
                for part in parts {
                    let StringPart::Text(span) = part else {
                        return Ok(None);
                    };
                    decode_text(source_text, *span, &mut bytes)?;
                }
                Ok(Some(bytes))
            }
            _ => Ok(None),
        }
    }

    fn decode_text(
        source_text: &str,
        span: Span,
        output: &mut Vec<u8>,
    ) -> Result<(), LoweringError> {
        let text = source_text
            .get(span.start()..span.end())
            .ok_or_else(|| error(LoweringErrorKind::MismatchedSource, span))?;
        let mut chars = text.chars();
        while let Some(character) = chars.next() {
            if character != '\\' {
                let mut encoded = [0_u8; 4];
                output.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
                continue;
            }
            let escaped = chars
                .next()
                .ok_or_else(|| error(LoweringErrorKind::InvalidLiteral, span))?;
            output.push(match escaped {
                '\\' => b'\\',
                '\'' => b'\'',
                '"' => b'"',
                'n' => b'\n',
                'r' => b'\r',
                't' => b'\t',
                '0' => b'\0',
                '$' => b'$',
                _ => return Err(error(LoweringErrorKind::InvalidLiteral, span)),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoweringErrorKind {
    MismatchedSource,
    MismatchedAnalysis,
    FrontendDiagnostics,
    BlockingDeferred,
    InstanceLimitExceeded,
    UnsupportedNode,
    UnsupportedCompanionCall,
    MissingFact,
    InvalidLiteral,
    InvalidModel,
    InvalidSsa,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LoweringError {
    pub(crate) kind: LoweringErrorKind,
    pub(crate) span: Option<Span>,
}

pub(in crate::ssa) const fn error(kind: LoweringErrorKind, span: Span) -> LoweringError {
    LoweringError {
        kind,
        span: Some(span),
    }
}

pub(in crate::ssa) const fn present_name(marker: NameMarker) -> Option<Span> {
    match marker {
        NameMarker::Present(span) => Some(span),
        NameMarker::Missing(_) | NameMarker::Error(_) => None,
    }
}

pub(in crate::ssa) const fn span_key(span: Span) -> (usize, usize) {
    (span.start(), span.end())
}

pub(in crate::ssa) fn parse_integer_literal_text(
    text: &str,
    kind: IntegerLiteralKind,
    span: Span,
) -> Result<i128, LoweringError> {
    lang_frontend::type_checking::integer_literal_magnitude(text, kind)
        .and_then(|value| i128::try_from(value).ok())
        .ok_or_else(|| error(LoweringErrorKind::InvalidLiteral, span))
}

pub(in crate::ssa) fn is_null_literal(parsed: &ParsedFile, mut expression: ExpressionId) -> bool {
    while let Ok(node) = parsed.ast().expressions().get(expression) {
        match node.payload() {
            Expression::Literal(LiteralKind::Null) => return true,
            Expression::Group { expression: inner } => expression = *inner,
            _ => return false,
        }
    }
    false
}

pub(in crate::ssa) fn parsed_by_source_unit<'a>(
    inputs: &'a [SourceUnitInput<'a>],
    names: &ValidatedCompilationUnitNames,
) -> Result<Vec<&'a ParsedFile>, LoweringError> {
    names
        .names()
        .index()
        .source_units()
        .iter()
        .map(|source_unit| {
            inputs
                .iter()
                .copied()
                .find(|input| input.source_id() == source_unit.source_id())
                .map(SourceUnitInput::parsed)
                .ok_or(LoweringError {
                    kind: LoweringErrorKind::MismatchedSource,
                    span: None,
                })
        })
        .collect()
}

/// Return the typed receiver operand only when it cannot reach receiver delivery.
pub(in crate::ssa) fn non_completing_call_receiver(
    typed: &CompilationUnitTypes,
    call: UnitExpressionId,
) -> Option<UnitExpressionId> {
    let UnitCallReceiverOrigin::Expression(receiver) = typed.call(call)?.receiver()?.origin()
    else {
        return None;
    };
    (typed.expression_falls_through(receiver) == Some(false)).then_some(receiver)
}
