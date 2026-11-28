//! 具名函数头部；扩展 receiver 复用正式 TypeRef，不创建第二套类型语法。
use super::*;

#[inline]
pub(super) const fn is_extension_receiver_follow_kind(kind: LexemeKind) -> bool {
    matches!(
        kind,
        LexemeKind::Token(TokenKind::Symbol(
            Symbol::Dot | Symbol::Less | Symbol::Question
        ))
    )
}

#[inline]
pub(super) const fn is_extension_receiver_head_kind(
    current_is_identifier: bool,
    next1: Option<LexemeKind>,
) -> bool {
    current_is_identifier
        && match next1 {
            Some(kind) => is_extension_receiver_follow_kind(kind),
            None => false,
        }
}

impl Parser<'_> {
    pub(super) fn parse_function_head(
        &mut self,
        stops: Stops,
    ) -> Result<(NameMarker, Option<ExtensionReceiverSyntax>), ParserInternalError> {
        let current_is_identifier = self.current_is_identifier();
        let next1 = if current_is_identifier {
            self.peek(1).map(Lexeme::kind)
        } else {
            None
        };
        let extension = is_extension_receiver_head_kind(current_is_identifier, next1);
        if !extension {
            let name = self.parse_name_marker(
                codes::EXPECTED_DECLARATION_NAME,
                "expected declaration name",
                NameContext::Declaration,
            )?;
            return Ok((name, None));
        }
        let type_ref = self.parse_type_ref(
            TypeStops::from_expression(stops)
                .with(TypeStops::RIGHT_PAREN)
                .with(TypeStops::EXTENSION_NAME),
        )?;
        let dot_span = if self.current_is_symbol(Symbol::Dot) {
            self.bump()?.span()
        } else {
            let span = self.empty_at(self.current()?.span().start())?;
            self.emit(
                codes::EXPECTED_DECLARATION_NAME,
                "expected '.' before extension function name",
                span,
            )?;
            span
        };
        let name = self.parse_name_marker(
            codes::EXPECTED_DECLARATION_NAME,
            "expected extension function name",
            NameContext::Declaration,
        )?;
        Ok((name, Some(ExtensionReceiverSyntax { type_ref, dot_span })))
    }
}
