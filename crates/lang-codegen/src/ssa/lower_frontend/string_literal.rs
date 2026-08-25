//! 编译器封闭 effect 使用的静态 String literal 解码。

use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, ParsedFile, StringPart},
    source::Span,
};

use super::{LoweringError, LoweringErrorKind, error};

pub(super) fn decode_plain(
    parsed: &ParsedFile,
    source_text: &str,
    expression: ExpressionId,
) -> Result<Option<Vec<u8>>, LoweringError> {
    let node = parsed
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

fn decode_text(source_text: &str, span: Span, output: &mut Vec<u8>) -> Result<(), LoweringError> {
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
